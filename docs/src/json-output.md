# JSON output

When built with the `json` feature (`cargo build --features json`), some
utilities accept `--output-format=json` to print their output as JSON;
`--output-format=text` gives the regular output back, the last one wins.

The keys are those of [jc](https://github.com/kellyjonbrazil/jc), so
`cmd --output-format=json` can replace `cmd | jc --cmd`. As with jc, an
option that adds or removes a column adds or removes a key. Unlike the text,
sizes are exact byte counts. The JSON is indented on a terminal and on one
line otherwise.

## `ls`

An array with one object per listed entry: `filename`, plus, for a long
listing, `link_to`, `flags`, `links`, `owner`, `group`, `size`, `date` (as
printed) and `epoch`.

```
$ ls -l --output-format=json
[{"filename":"notes.txt","flags":"-rw-r--r--","links":1,"owner":"alice","group":"staff","size":42,"date":"Nov 19 18:27","epoch":1795109278}]
```

## `uname`

An object with the printed fields: `kernel_name`, `node_name`,
`kernel_release`, `kernel_version`, `machine`, `processor`,
`hardware_platform` and `operating_system`.

```
$ uname -sm --output-format=json
{"kernel_name":"Linux","machine":"x86_64"}
```

## `df`

An array with one object per row, the `--total` row included, with one key
per column: `filesystem`, `type`, `size`, `used`, `available`, `use_percent`,
`inodes`, `iused`, `ifree`, `iused_percent`, `file` and `mounted_on`.

```
$ df / --output-format=json
[{"filesystem":"/dev/sda2","size":1005441241088,"used":939699818496,"available":14592307200,"use_percent":99,"mounted_on":"/"}]
```

## `du`

An array with one `{"size", "name"}` object per line, `size` in bytes (or
inodes with `--inodes`).

```
$ du -s src --output-format=json
[{"size":118784,"name":"src"}]
```

## `wc`

An array with one object per line, the total included: `filename`, `lines`,
`words`, `characters` (`-m`), `bytes` and `max_line_length`.

```
$ wc --output-format=json notes.txt
[{"filename":"notes.txt","lines":3,"words":12,"bytes":61}]
```

## `env`

An array with one `{"name", "value"}` object per variable.

```
$ env --output-format=json -i LANG=C
[{"name":"LANG","value":"C"}]
```
