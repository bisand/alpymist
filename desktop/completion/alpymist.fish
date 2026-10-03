# Tab after `alpymist` in fish. What is offered is `alpymist complete`'s to
# say (crates/alpymist/src/complete.rs): a value on each line, then a tab and
# what it is, as fish takes them. It ends with 3 when the word is a file's
# name.

function __alpymist_complete
    # Quoted: a word not yet begun is an empty word, not no word.
    set -l current (commandline -ct)
    set -l words (commandline -opc) "$current"
    alpymist complete -- $words[2..] 2>/dev/null
    if test $status -eq 3
        __fish_complete_path "$current"
    end
end

complete -c alpymist -f -a '(__alpymist_complete)'
