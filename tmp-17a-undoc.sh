#!/bin/sh
# Lists item declarations whose previous non-attribute line is not a doc comment.
set -u

for file in "$@"; do
    previous=''
    number=0
    while IFS= read -r text; do
        number=$((number + 1))
        case $text in
            *'#['*) continue ;;
        esac
        case $text in
            *'pub fn '* | *'pub struct '* | *'pub enum '* | *'pub const '* | \
                *'pub(crate) fn '* | *'pub(crate) struct '* | *'pub(crate) enum '* | \
                'fn '* | 'struct '* | 'enum '* | 'const '* | '    fn '*)
                case $previous in
                    *'///'*) ;;
                    *) echo "$file:$number: $text" ;;
                esac
                ;;
        esac
        previous=$text
    done < "$file"
done
