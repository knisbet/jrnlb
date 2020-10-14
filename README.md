# jrnlb
Utility to directly read journal export format files without needing the long process to re-import the export back into the systemd journal. Partially re-implements the journalctl CLI to allow filtering the results, but presently isn't a full implementation.

## Status
Experimental

Todos:
- [ ] Allowing reading from stdin (`cat <file> | jrnlb`)
- [ ] Implement more export formats. Currently not faithful to any format.
- [ ] Implement cursor support.
- [ ] Implement No Hostname Output
- [ ] Allow selection of tracked fields and customize output to requested field list
- [x] Implement Since / Until time filters
- [x] Limit the output to `n` lines
- [x] Support gzip compressed files directly without decompression
- [ ] Create / Publish docker container with the utility
- [ ] Consider implementing caching, to speed up subsequent reads of the same file (if needed)

## Install
### Cargo
If you have the rust toolchain available, install via cargo:

`cargo install --git https://github.com/knisbet/jrnlb`

## Usage
```
jrnlb 0.1.0
Kevin Nisbet <kevin@xybyte.com>
This doc string acts as a help message when the user runs '--help' as do all doc strings on fields

USAGE:
    jrnlb [OPTIONS] [files]...

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -n, --lines <lines>           Number of journal entries to show
    -o, --output <output-mode>    Change journal output mode [possible values: short, short_precise, short_iso,
                                  short_iso_precise, short_full, short_monotonic, short_unix, verbose, export, json,
                                  json_pretty, json_sse, json_seq, cat, with_unit]
    -S, --since <since>           Show entries not older than the specified date
    -u, --unit <unit>             Show logs from the specified unit
    -U, --until <until>           Show entries not newer than the specified date

ARGS:
    <files>...    Journal export files to parse
```

## Example
```
# Export some log entries from the journal
> journalctl -n 5 -o export > /tmp/journal.export    

# Inspect those log entries
> jrnlb /tmp/journal.export       
2020-10-14T04:54:59.000140546+00:00 knisbet-dev sshd[5605]: Disconnected from authenticating user root 80.211.56.216 port 39400 [preauth]
2020-10-14T04:54:59.000421522+00:00 knisbet-dev sshg-blocker[803]: Attack from "80.211.56.216" on service 100 with danger 10.
2020-10-14T04:54:59.000921350+00:00 knisbet-dev sshg-blocker[803]: Attack from "80.211.56.216" on service 110 with danger 10.
2020-10-14T04:55:00.000421362+00:00 knisbet-dev sshg-blocker[803]: Attack from "80.211.56.216" on service 110 with danger 10.
2020-10-14T04:55:00.000421388+00:00 knisbet-dev sshg-blocker[803]: Blocking "80.211.56.216/32" for 120 secs (3 attacks in 1 secs, after 1 abuses over 1 secs.)

# Limit Output
❯ jrnlb /tmp/journal.export -n 1
Opts { filter: Filter { unit: None, since: None, until: None, lines: Some(1) }, files: ["/tmp/journal.export"], output_mode: None }
2020-10-14T04:54:59.000140546+00:00 knisbet-dev sshd[5605]: Disconnected from authenticating user root 80.211.56.216 port 39400 [preauth]

```

