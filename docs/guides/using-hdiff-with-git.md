# Use hdiff for one Git diff

Install hdiff as the current user's default Git difftool:

```sh
hdiff --install
```

The command copies the exact global Git configuration file Git will edit to a sibling `.bak`
file, then configures `git difftool` to start the current hdiff executable with Git's temporary
before and after files. It overwrites only `diff.tool` and `difftool.hdiff.cmd`.

Run `git difftool` to review a change:

```sh
git difftool HEAD~3
```

Run the repository wrapper with any `git diff` arguments when a one-shot pager is more useful:

```sh
./git.sh diff HEAD~3
```

The wrapper builds the release binary and supplies it as Git's pager for that invocation
only. It does not change Git configuration.

This is a pager integration: hdiff receives the unified diff on standard input.
