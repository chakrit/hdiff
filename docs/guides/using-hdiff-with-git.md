# Use hdiff for one Git diff

Install hdiff as the current user's `git diff` pager:

```sh
hdiff --install
```

The command copies the exact global Git configuration file Git will edit to a sibling `.bak`
file, then configures `pager.diff` to start the current hdiff executable. It overwrites only
`pager.diff`; general Git paging through `core.pager` remains unchanged.
Afterward, it prints a shell-safe `cp` command when it created a backup and the `git config`
command it applied.

Run `git diff` to review every changed file in one hdiff session:

```sh
git diff HEAD~3
```

Run the repository wrapper when a one-shot pager is more useful:

```sh
./git.sh diff HEAD~3
```

The wrapper builds the release binary and supplies it as Git's pager for that invocation
only. It does not change Git configuration.

This is a pager integration: hdiff receives the unified diff on standard input.
