hotpatch:
   dx serve --hot-patch --features hotpatching

run:
   cargo run

debug:
   cargo run --features debug

push:
   jj b m main
   jj git push