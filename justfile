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