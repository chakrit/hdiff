# Use hdiff for one Git diff

Run the repository wrapper with any `git diff` arguments:

```sh
./git.sh diff HEAD~3
```

The wrapper builds the release binary and supplies it as Git's pager for that invocation
only. It does not change Git configuration.

This is a pager integration: hdiff receives the unified diff on standard input. It is not
a `git difftool` integration, which requires support for Git's two-file tool contract.
