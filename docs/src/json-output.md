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
