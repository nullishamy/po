# po

a simple photo organiser.

usage:
```console
po --config po.toml
```
basic config:
```toml
inputs = [ "input" ]
output = "sorted"
extensions = [ "cr2", "jpeg" ]
sort_policy = "Date"
```

po stores its metadata in `<outputdir>/_pometa`, any manual changes to this directory risk corrupting the library etc.

when po is run, it will automatically sort everything according to the config. it will leave any excluded or duplicated (as determined by content hash) files where it found them.
