# Kubernetes preparation benchmark

Benchmark `hdiff` preparation against Go-only changes from a pinned Kubernetes commit.

Set `KUBERNETES` to a local `kubernetes/kubernetes` checkout and `PINNED_COMMIT` to the full
commit identifier being measured. Record that identifier, each resolved base/target pair, and the
single `hdiff --bench` result line for every range.

```sh
cd "$KUBERNETES"
git rev-list --first-parent --reverse "$PINNED_COMMIT" > /tmp/hdiff-first-parent.txt
edge_count=$(($(wc -l < /tmp/hdiff-first-parent.txt) - 1))

for numerator in 7 6 4; do
  base_index=$((edge_count * numerator / 8))
  base_commit=$(sed -n "$((base_index + 1))p" /tmp/hdiff-first-parent.txt)
  printf 'base=%s target=%s\n' "$base_commit" "$PINNED_COMMIT"
  git diff "$base_commit" "$PINNED_COMMIT" -- '*.go' | hdiff --bench
done
```

The three numerators select the final eighth, quarter, and half of first-parent history.
