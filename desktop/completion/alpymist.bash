# Tab after `alpymist` in bash. What is offered is `alpymist complete`'s to
# say (crates/alpymist/src/complete.rs): a value on each line, then a tab and
# what it is, which bash has nowhere to show. It ends with 3 when the word is
# a file's name.

_alpymist() {
    local out line
    COMPREPLY=()
    out=$(alpymist complete -- "${COMP_WORDS[@]:1:COMP_CWORD}" 2>/dev/null)
    if [ $? -eq 3 ]; then
        compopt -o filenames 2>/dev/null
        while IFS= read -r line; do
            COMPREPLY+=("$line")
        done < <(compgen -f -- "${COMP_WORDS[COMP_CWORD]}")
        return
    fi
    while IFS= read -r line; do
        [ -n "$line" ] && COMPREPLY+=("${line%%$'\t'*}")
    done <<< "$out"
}

complete -F _alpymist alpymist
