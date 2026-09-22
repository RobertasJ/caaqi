[default]
run:
    cargo run

push:
    jj b m main
    jj git push

[arg("pager", long, value="true")]
test test_name="" pager="false":
    cargo test -q {{ test_name }} -p caaqi --lib {{ if pager == "true" { " | less -R" } else { "" } }}

update-state:
    @jj git fetch
    @snapshot=$(jj log -r @ --no-graph -T 'commit_id'); \
    jj rebase -r @ -d main; \
    jj restore --from "$snapshot"
