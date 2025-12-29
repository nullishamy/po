# po

a simple photo organiser.

usage:
```console
// po defaults to importing using the config file
po --config po.toml

// you can query your library too, with globs, as your library is just on the fs
// for example, to get all files from this year
po --config po.toml query "2025/**"

// you can also set your config file via env
PO_CONFIG_PATH=po.toml po query "2025/**"
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
