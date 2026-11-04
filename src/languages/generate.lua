-- Generates hdiff's language data from Neovim's filetype tables and the nvim-treesitter
-- submodule: `file_types.rs`, `grammars.rs` and `queries/<grammar>.scm` beside this script.
-- Run by a maintainer from the repository root; building and running hdiff need no Neovim:
--
--   nvim --clean --headless -l src/languages/generate.lua
--
-- It also moves each grammar submodule under `vendor/` to nvim-treesitter's pinned
-- revision, and prints a report of everything it dropped or could not resolve.
-- Rules: docs/spec/architecture.md#technology-direction.

-- The nvim-treesitter grammars compiled into hdiff.
local STARTER = { 'go' }

-- hdiff's own entries, which win over Neovim's: fixed file types for names Neovim detects
-- by content, names Neovim selects only by full path or does not know, and names whose
-- mainstream language hdiff prefers over Neovim's pick.
local HDIFF_ENTRIES = {
  extension = {
    asm = 'asm',
    containerfile = 'dockerfile',
    gsh = 'groovy',
    gvy = 'groovy',
    gy = 'groovy',
    h = 'c',
    ['h++'] = 'cpp',
    inf = 'dosini',
    m = 'objc',
    make = 'make',
    mdx = 'markdown',
    mm = 'objcpp',
    mount = 'systemd',
    nomad = 'hcl',
    phps = 'php',
    plist = 'xml',
    props = 'xml',
    resx = 'xml',
    S = 'asm',
    s = 'asm',
    service = 'systemd',
    slice = 'systemd',
    socket = 'systemd',
    t = 'perl',
    target = 'systemd',
    targets = 'xml',
    tf = 'terraform',
    timer = 'systemd',
    tpp = 'cpp',
    xaml = 'xml',
  },
  filename = {
    ['.envrc'] = 'sh',
    ['.flake8'] = 'dosini',
    ['.terraformrc'] = 'hcl',
    ['Capfile'] = 'ruby',
    ['Fastfile'] = 'ruby',
    ['Guardfile'] = 'ruby',
    ['Podfile'] = 'ruby',
    ['poetry.lock'] = 'toml',
    ['terraform.rc'] = 'hcl',
  },
}

local NVIM_TREESITTER = 'vendor/nvim-treesitter'
local OUTPUT = 'src/languages'

-- Predicates the `tree-sitter` crate evaluates, and the `#lua-match?` forms it reads as
-- regex matches once the Lua pattern is translated.
local STANDARD_PREDICATES = {
  ['eq?'] = true,
  ['not-eq?'] = true,
  ['any-eq?'] = true,
  ['any-not-eq?'] = true,
  ['match?'] = true,
  ['not-match?'] = true,
  ['any-match?'] = true,
  ['any-not-match?'] = true,
  ['any-of?'] = true,
  ['not-any-of?'] = true,
  ['set!'] = true,
}
local LUA_MATCHES = {
  ['lua-match?'] = 'match?',
  ['not-lua-match?'] = 'not-match?',
  ['any-lua-match?'] = 'any-match?',
  ['any-not-lua-match?'] = 'any-not-match?',
}
local REGEX_MATCHES = {
  ['match?'] = true,
  ['not-match?'] = true,
  ['any-match?'] = true,
  ['any-not-match?'] = true,
}

local report = {}

local function note(line)
  table.insert(report, line)
end

-- Running commands

local function run(command)
  local result = vim.system(command, { text = true }):wait()
  if result.code ~= 0 then
    error(table.concat(command, ' ') .. ' failed:\n' .. result.stderr)
  end
  return vim.trim(result.stdout)
end

local function read(path)
  local file = assert(io.open(path, 'rb'))
  local text = file:read('a')
  file:close()
  return text
end

local function write(path, text)
  local file = assert(io.open(path, 'wb'))
  file:write(text)
  file:close()
end

-- Sorting by bytes, the order Rust compares `str` in.

local function bytes_before(left, right)
  local length = math.min(#left, #right)
  for index = 1, length do
    local left_byte, right_byte = left:byte(index), right:byte(index)
    if left_byte ~= right_byte then
      return left_byte < right_byte
    end
  end
  return #left < #right
end

local function sorted_keys(map)
  local keys = vim.tbl_keys(map)
  table.sort(keys, bytes_before)
  return keys
end

-- File types

-- The file type a filetype-table entry gives the file named `file_name`: the entry's own
-- string, or Neovim's answer by name alone for an entry that detects by content.
local function resolve(entry, file_name)
  local value = type(entry) == 'table' and entry[1] or entry
  if type(value) == 'string' then
    return value
  end
  return vim.filetype.match({ filename = file_name })
end

-- The literal file names a Lua pattern's body stands for, expanding letter sets such as
-- `[mM]`; nil when the body uses any other pattern feature.
local function literal_expansions(body)
  local expansions = { '' }
  local index = 1
  while index <= #body do
    local char = body:sub(index, index)
    local choices
    if char == '%' and body:sub(index + 1, index + 1):match('%p') then
      choices, index = { body:sub(index + 1, index + 1) }, index + 2
    elseif char == '[' then
      local set = body:match('^%[(%w+)%]', index)
      if not set then
        return nil
      end
      choices, index = vim.split(set, ''), index + #set + 2
    elseif char:match('[%^%$%(%)%.%%%[%]%*%+%-%?]') then
      return nil
    else
      choices, index = { char }, index + 1
    end
    local grown = {}
    for _, prefix in ipairs(expansions) do
      for _, choice in ipairs(choices) do
        table.insert(grown, prefix .. choice)
      end
    end
    expansions = grown
  end
  return expansions
end

local SHAPES = {
  [true] = { [true] = 'Exact', [false] = 'Prefix' },
  [false] = { [true] = 'Suffix', [false] = 'Contains' },
}

-- The name patterns a Neovim filetype pattern becomes, or nil when it reads the full path
-- or is more than a literal anchored at either end.
local function name_patterns(pattern, entry)
  if pattern:find('/') then
    return nil
  end
  local starts = pattern:sub(1, 1) == '^'
  local ends = pattern:sub(-1) == '$' and pattern:sub(-2, -2) ~= '%'
  local body = pattern:sub(starts and 2 or 1, ends and -2 or -1)
  local literals = literal_expansions(body)
  if not literals then
    return nil
  end

  local options = type(entry) == 'table' and entry[2] or {}
  local results = {}
  for _, literal in ipairs(literals) do
    local file_type = resolve(entry, literal)
    if file_type then
      table.insert(results, {
        shape = SHAPES[starts][ends],
        literal = literal,
        file_type = file_type,
        priority = options.priority or 0,
      })
    end
  end
  return results
end

-- Neovim's order: higher priority first; ties in pattern byte order, so output is stable.
local function pattern_before(left, right)
  if left.priority ~= right.priority then
    return left.priority > right.priority
  end
  return bytes_before(left.literal, right.literal)
end

local function patterns(tables)
  local first, last = {}, {}
  local dropped = 0
  for _, group in pairs(tables.pattern) do
    for pattern, entry in pairs(group) do
      local results = name_patterns(pattern, entry)
      if not results then
        dropped = dropped + 1
      end
      for _, result in ipairs(results or {}) do
        table.insert(result.priority >= 0 and first or last, result)
      end
    end
  end

  table.sort(first, pattern_before)
  table.sort(last, pattern_before)
  note(
    ('dropped %d filetype patterns that read the full path or are not literal'):format(
      dropped
    )
  )
  return first, last
end

local function file_types()
  local tables = vim.filetype.inspect()
  local resolved = { extension = {}, filename = {} }
  local unresolved = {}
  for kind, entries in pairs(resolved) do
    for name, entry in pairs(tables[kind]) do
      local file_name = kind == 'extension' and ('file.' .. name) or name
      entries[name] = resolve(entry, file_name)
      if not entries[name] and not HDIFF_ENTRIES[kind][name] then
        table.insert(unresolved, name)
      end
    end
    for name, file_type in pairs(HDIFF_ENTRIES[kind]) do
      entries[name] = file_type
    end
  end

  table.sort(unresolved, bytes_before)
  note('content-detected names with no default: ' .. table.concat(unresolved, ' '))
  resolved.first, resolved.last = patterns(tables)
  return resolved
end

-- Lua patterns to regex

local LUA_CLASSES = {
  a = 'a-zA-Z',
  c = '\\x00-\\x1f\\x7f',
  d = '0-9',
  l = 'a-z',
  p = '!-/:-@\\[-`{-~',
  s = '\\s',
  u = 'A-Z',
  w = 'a-zA-Z0-9',
  x = '0-9a-fA-F',
}

local function escape_regex(char)
  if char:match('[%w%s_]') then
    return char
  end
  return '\\' .. char
end

-- The regex text of the Lua class `%<letter>` at `letter`, inside a bracket set or not.
local function translate_class(letter, in_set)
  local class = LUA_CLASSES[letter:lower()]
  if not class then
    return escape_regex(letter)
  end
  if letter == letter:lower() then
    return in_set and class or ('[' .. class .. ']')
  end
  if in_set then
    return nil
  end
  return '[^' .. class .. ']'
end

-- The regex text of the bracket set starting at `start`, and the index after it.
local function translate_set(pattern, start)
  local parts = { '[' }
  local index = start + 1
  if pattern:sub(index, index) == '^' then
    table.insert(parts, '^')
    index = index + 1
  end

  local first = true
  while index <= #pattern do
    local char = pattern:sub(index, index)
    if char == ']' and not first then
      table.insert(parts, ']')
      return table.concat(parts), index + 1
    end
    if char == '%' then
      local class = translate_class(pattern:sub(index + 1, index + 1), true)
      if not class then
        return nil
      end
      table.insert(parts, class)
      index = index + 2
    else
      table.insert(parts, (char == '-' and not first) and '-' or escape_regex(char))
      index = index + 1
    end
    first = false
  end
  return nil
end

local QUANTIFIERS = { ['*'] = '*', ['+'] = '+', ['?'] = '?', ['-'] = '*?' }

-- The Rust regex that matches what the Lua pattern `pattern` matches, or nil when the
-- pattern uses a feature regex lacks (`%b`, `%f`, back-references, position captures).
local function lua_pattern_to_regex(pattern)
  local parts = {}
  local index = 1
  local after_item = false
  while index <= #pattern do
    local char = pattern:sub(index, index)
    local quantifier = QUANTIFIERS[char]
    local piece
    local next_index = index + 1
    local item = true
    if quantifier and after_item then
      piece, item = quantifier, false
    elseif char == '^' and index == 1 then
      piece, item = '^', false
    elseif char == '$' and index == #pattern then
      piece, item = '$', false
    elseif char == '.' then
      piece = '.'
    elseif char == '[' then
      piece, next_index = translate_set(pattern, index)
    elseif char == '%' then
      local letter = pattern:sub(index + 1, index + 1)
      if letter:match('[bf%d]') then
        return nil
      end
      piece, next_index = translate_class(letter, false), index + 2
    elseif char == '(' then
      if pattern:sub(index + 1, index + 1) == ')' then
        return nil
      end
      piece, item = '(', false
    elseif char == ')' then
      piece = ')'
    else
      piece = escape_regex(char)
    end
    if not piece then
      return nil
    end
    table.insert(parts, piece)
    index, after_item = next_index, item
  end
  return table.concat(parts)
end

-- Queries

local function unquote(literal)
  local escapes = { n = '\n', t = '\t', r = '\r' }
  return (
    literal:sub(2, -2):gsub('\\(.)', function(char)
      return escapes[char] or char
    end)
  )
end

local function quote(text)
  return '"' .. text:gsub('\\', '\\\\'):gsub('"', '\\"') .. '"'
end

-- Vim-regex escapes whose meaning differs in Rust's regex, which Neovim's `#match?` reads.
local function vim_only_regex(regex)
  return regex:find('\\[^dDsSwWbBntr%.\\%[%]%^%$%*%+%-/]') ~= nil
end

local predicate_query = vim.treesitter.query.parse('query', '(predicate) @predicate')

-- A predicate's identifier; its `name` field also holds the leading `#`.
local function predicate_identifier(node)
  for _, child in ipairs(node:field('name')) do
    if child:type() == 'identifier' then
      return child
    end
  end
end

-- The replacements one predicate needs, or the reason its pattern is dropped.
local function rewrite_predicate(node, text)
  local identifier = predicate_identifier(node)
  local name = vim.treesitter.get_node_text(identifier, text)
    .. vim.treesitter.get_node_text(node:field('type')[1], text)
  local translated = LUA_MATCHES[name]
  if translated then
    local edits = { { identifier, translated:sub(1, -2) } }
    for parameter in node:field('parameters')[1]:iter_children() do
      if parameter:type() == 'string' then
        local pattern = unquote(vim.treesitter.get_node_text(parameter, text))
        local regex = lua_pattern_to_regex(pattern)
        if not regex then
          return nil, ('#%s %q has no regex form'):format(name, pattern)
        end
        table.insert(edits, { parameter, quote(regex) })
      end
    end
    return edits
  end

  if not STANDARD_PREDICATES[name] then
    return nil, '#' .. name
  end
  if REGEX_MATCHES[name] then
    for parameter in node:field('parameters')[1]:iter_children() do
      local is_string = parameter:type() == 'string'
      if
        is_string
        and vim_only_regex(unquote(vim.treesitter.get_node_text(parameter, text)))
      then
        return nil, '#' .. name .. ' with Vim regex'
      end
    end
  end
  return {}
end

-- `pattern`'s text with its predicates rewritten, or nil and the reason it is dropped.
local function translate_pattern(pattern, text)
  local edits = {}
  for _, node in predicate_query:iter_captures(pattern, text) do
    local predicate_edits, reason = rewrite_predicate(node, text)
    if not predicate_edits then
      return nil, reason
    end
    vim.list_extend(edits, predicate_edits)
  end

  local _, _, start = pattern:start()
  local pieces, cursor = {}, start
  table.sort(edits, function(left, right)
    return select(3, left[1]:start()) < select(3, right[1]:start())
  end)
  for _, edit in ipairs(edits) do
    local _, _, from = edit[1]:start()
    local _, _, to = edit[1]:end_()
    table.insert(pieces, text:sub(cursor + 1, from))
    table.insert(pieces, edit[2])
    cursor = to
  end
  local _, _, finish = pattern:end_()
  table.insert(pieces, text:sub(cursor + 1, finish))
  return table.concat(pieces)
end

local function query_path(language)
  return ('%s/runtime/queries/%s/highlights.scm'):format(NVIM_TREESITTER, language)
end

-- The parents named on `text`'s `; inherits:` line; a parenthesized parent is read only
-- when this query is itself inherited.
local function parents(text, inherited)
  local line = text:match('^;+%s*inherits%s*:?%s*([^\n]*)') or ''
  local names = {}
  for name in line:gmatch('[^,%s]+') do
    local optional = name:match('^%((.*)%)$')
    if not optional then
      table.insert(names, name)
    elseif inherited then
      table.insert(names, optional)
    end
  end
  return names
end

-- `language`'s highlight query with its parents before it, every pattern's predicates
-- translated, and every pattern the `tree-sitter` crate cannot evaluate dropped.
local function translate_query(language, inherited)
  local text = read(query_path(language))
  local root = vim.treesitter.get_string_parser(text, 'query'):parse()[1]:root()
  local sections = {}
  for _, parent in ipairs(parents(text, inherited)) do
    table.insert(sections, translate_query(parent, true))
  end

  local dropped = {}
  for pattern in root:iter_children() do
    if pattern:type() ~= 'comment' then
      local translated, reason = translate_pattern(pattern, text)
      if translated then
        table.insert(sections, translated)
      else
        dropped[reason] = (dropped[reason] or 0) + 1
      end
    end
  end

  for _, reason in ipairs(sorted_keys(dropped)) do
    note(
      ('query %s: dropped %d patterns using %s'):format(language, dropped[reason], reason)
    )
  end
  return table.concat(sections, '\n\n')
end

-- Grammar submodules

-- Moves the submodule at `directory` to `revision`, adding it first when it is missing.
local function pin_submodule(url, directory, revision)
  if vim.uv.fs_stat(directory .. '/.git') == nil then
    run({ 'git', 'submodule', 'add', url, directory })
    run({
      'git',
      'config',
      '-f',
      '.gitmodules',
      'submodule.' .. directory .. '.shallow',
      'true',
    })
    note(('added submodule %s'):format(directory))
  end

  local current = run({ 'git', '-C', directory, 'rev-parse', 'HEAD' })
  if current ~= revision then
    run({ 'git', '-C', directory, 'fetch', '--depth', '1', 'origin', revision })
    run({ 'git', '-C', directory, 'checkout', '--quiet', revision })
    note(
      ('moved %s from %s to %s'):format(directory, current:sub(1, 7), revision:sub(1, 7))
    )
  end
end

local function manifest_field(directory, field)
  return read(directory .. '/Cargo.toml'):match('\n' .. field .. '%s*=%s*"([^"]*)"')
end

-- The starter grammar `name`, pinned and described, or nil when hdiff cannot compile it.
local function grammar(name, parsers, filetypes)
  local parser = parsers[name]
  if not parser then
    note(('grammar %s: not in nvim-treesitter'):format(name))
    return nil
  end
  local install = parser.install_info
  if install.generate then
    note(('grammar %s: commits no generated parser; scanner only'):format(name))
    return nil
  end

  local repository = install.url:match('([^/]+)$')
  local directory = 'vendor/' .. repository
  pin_submodule(install.url, directory, install.revision)
  local crate = manifest_field(directory, 'name')
  if not crate then
    note(('grammar %s: %s has no Cargo manifest; scanner only'):format(name, directory))
    return nil
  end

  local selected = {}
  for _, filetype in ipairs(filetypes) do
    if vim.treesitter.language.get_lang(filetype) == name then
      table.insert(selected, filetype)
    end
  end
  return {
    name = name,
    crate = crate,
    directory = directory,
    revision = install.revision,
    license = manifest_field(directory, 'license') or 'unknown',
    file_types = selected,
    query = translate_query(name, false),
  }
end

-- Output

local function header(neovim, nvim_treesitter)
  return (
    '// @generated by src/languages/generate.lua from Neovim %s and nvim-treesitter %s;\n'
    .. '// do not edit.\n'
  ):format(neovim, nvim_treesitter:sub(1, 12))
end

local function rust_string(text)
  return '"' .. text:gsub('\\', '\\\\'):gsub('"', '\\"') .. '"'
end

local function rust_pairs(entries)
  local lines = {}
  for _, key in ipairs(sorted_keys(entries)) do
    table.insert(
      lines,
      ('    (%s, %s),'):format(rust_string(key), rust_string(entries[key]))
    )
  end
  return table.concat(lines, '\n')
end

local function rust_patterns(patterns)
  local lines = {}
  for _, pattern in ipairs(patterns) do
    local line = ('    (NamePattern::%s(%s), %s),'):format(
      pattern.shape,
      rust_string(pattern.literal),
      rust_string(pattern.file_type)
    )
    table.insert(lines, line)
  end
  return table.concat(lines, '\n')
end

local function file_types_source(resolved, preamble)
  return preamble
    .. '\nuse super::NamePattern;\n'
    .. '\n/// Whole file names and the file types they select, sorted by file name.\n'
    .. 'pub static FILE_NAMES: &[(&str, &str)] = &[\n'
    .. rust_pairs(resolved.filename)
    .. '\n];\n\n/// Name patterns tried after file names and before extensions, first match wins.\n'
    .. 'pub static PATTERNS_BEFORE_EXTENSIONS: &[(NamePattern, &str)] = &[\n'
    .. rust_patterns(resolved.first)
    .. '\n];\n\n/// File extensions and the file types they select, sorted by extension.\n'
    .. 'pub static EXTENSIONS: &[(&str, &str)] = &[\n'
    .. rust_pairs(resolved.extension)
    .. '\n];\n\n/// Name patterns tried after extensions, first match wins.\n'
    .. 'pub static PATTERNS_AFTER_EXTENSIONS: &[(NamePattern, &str)] = &[\n'
    .. rust_patterns(resolved.last)
    .. '\n];\n'
end

local function grammar_entry(compiled)
  local file_types = {}
  for _, file_type in ipairs(compiled.file_types) do
    table.insert(file_types, rust_string(file_type))
  end
  return table.concat({
    ('    // %s at %s, %s'):format(
      compiled.directory,
      compiled.revision:sub(1, 12),
      compiled.license
    ),
    '    Grammar {',
    ('        name: %s,'):format(rust_string(compiled.name)),
    ('        file_types: &[%s],'):format(table.concat(file_types, ', ')),
    ('        language: || %s::LANGUAGE.into(),'):format(compiled.crate:gsub('-', '_')),
    ('        highlights: include_str!("queries/%s.scm"),'):format(compiled.name),
    '    },',
  }, '\n')
end

local function grammars_source(compiled, preamble)
  local entries = {}
  for _, entry in ipairs(compiled) do
    table.insert(entries, grammar_entry(entry))
  end
  return preamble
    .. '\nuse super::Grammar;\n\n'
    .. '/// The grammars compiled into hdiff, with the file types each highlights.\n'
    .. 'pub static GRAMMARS: &[Grammar] = &[\n'
    .. table.concat(entries, '\n')
    .. '\n];\n'
end

local function query_source(compiled, nvim_treesitter)
  return (
    '; @generated by src/languages/generate.lua from nvim-treesitter %s\n'
    .. '; runtime/queries/%s/highlights.scm (Apache-2.0); do not edit.\n\n%s\n'
  ):format(nvim_treesitter:sub(1, 12), compiled.name, compiled.query)
end

local function main()
  vim.opt.runtimepath:prepend(NVIM_TREESITTER)
  dofile(NVIM_TREESITTER .. '/plugin/filetypes.lua')
  local parsers = dofile(NVIM_TREESITTER .. '/lua/nvim-treesitter/parsers.lua')
  local nvim_treesitter = run({ 'git', '-C', NVIM_TREESITTER, 'rev-parse', 'HEAD' })
  local version = vim.version()
  local neovim = ('v%d.%d.%d'):format(version.major, version.minor, version.patch)
  local preamble = header(neovim, nvim_treesitter)

  local resolved = file_types()
  local all_file_types = {}
  for _, entries in ipairs({ resolved.extension, resolved.filename }) do
    for _, file_type in pairs(entries) do
      all_file_types[file_type] = true
    end
  end
  for _, pattern in ipairs(vim.list_extend(vim.list_slice(resolved.first), resolved.last)) do
    all_file_types[pattern.file_type] = true
  end
  local compiled = {}
  for _, name in ipairs(STARTER) do
    table.insert(compiled, grammar(name, parsers, sorted_keys(all_file_types)))
  end

  vim.fn.mkdir(OUTPUT .. '/queries', 'p')
  write(OUTPUT .. '/file_types.rs', file_types_source(resolved, preamble))
  write(OUTPUT .. '/grammars.rs', grammars_source(compiled, preamble))
  for _, entry in ipairs(compiled) do
    write(
      ('%s/queries/%s.scm'):format(OUTPUT, entry.name),
      query_source(entry, nvim_treesitter)
    )
  end
  print(table.concat(report, '\n'))
end

main()
