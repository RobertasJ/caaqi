hotpatch:
   dx serve --hot-patch --features hotpatching

run:
   cargo run

run-example example:
   cargo run --example {{example}}

[default]
debug:
   cargo run --features debug

push:
   jj b m main
   jj git push

[arg("pager", long, value="true")]
test test_name="" pager="false":
   cargo test -q {{test_name}} -p bevy_caaqi --lib {{if pager == "true" {" | less -R"} else {""}}}

update-state:
    #!/usr/bin/env bash
    set -euo pipefail
    jj git fetch
    snapshot=$(jj log -r @ --no-graph -T 'commit_id')
    jj rebase -r @ -d main
    jj restore --from "$snapshot"