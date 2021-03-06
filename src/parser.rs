use nom::bytes::streaming::take;
use nom::{error::ErrorKind, Err, IResult};

#[cfg(test)]
use nom::Needed;
#[cfg(test)]
use pretty_assertions::assert_eq;

/* jourald export format rules
wo journal entries that follow each other are separated by a double newline.
Journal fields consisting only of valid non-control UTF-8 codepoints are serialized as they are (i.e. the field name, followed by '=', followed by field data), followed by a newline as separator to the next field. Note that fields containing newlines cannot be formatted like this. Non-control UTF-8 codepoints are the codepoints with value at or above 32 (' '), or equal to 9 (TAB).
Other journal fields are serialized in a special binary safe way: field name, followed by newline, followed by a binary 64bit little endian size value, followed by the binary field data, followed by a newline as separator to the next field.
Entry metadata that is not actually a field is serialized like it was a field, but beginning with two underscores. More specifically, __CURSOR=, __REALTIME_TIMESTAMP=, __MONOTONIC_TIMESTAMP= are introduced this way. Note that these meta-fields are only generated when actual journal files are serialized. They are omitted for entries that do not originate from a journal file (for example because they are transferred for the first time to be stored in one). Or in other words: if you are generating this format you shouldn't care about these special double-underscore fields. But you might find them usable when you deserialize the format generated by us. Additional fields prefixed with two underscores might be added later on, your parser should skip over the fields it does not know.
The order in which fields appear in an entry is undefined and might be different for each entry that is serialized. And that's already it.
*/

use nom::bytes::streaming::take_while1;

const EQUALS: u8 = b'=';
const NEWLINE: u8 = b'\n';

fn parse_key(s: &[u8]) -> IResult<&[u8], &[u8]> {
    take_while1(|c| c != EQUALS && c != NEWLINE)(s)
}

#[test]
fn parse_key_test() {
    assert_eq!(parse_key(b"latin=123"), Ok((&b"=123"[..], &b"latin"[..])));
    assert_eq!(parse_key(b"latin"), Err(Err::Incomplete(Needed::Size(1))));
    assert_eq!(
        parse_key(b"=123"),
        Err(Err::Error((&b"=123"[..], ErrorKind::TakeWhile1)))
    );
}

use nom::bytes::streaming::tag;
use nom::number::streaming::le_u64;
use nom::sequence::tuple;

fn parse_value(s: &[u8]) -> IResult<&[u8], &[u8]> {
    match s[0] {
        EQUALS => parse_value_string(s),
        NEWLINE => parse_value_binary(s),
        _ => Err(Err::Error((&b""[..], ErrorKind::Tag))),
    }
    //alt((parse_value_binary, parse_value_string))(s)
}
fn parse_value_binary(s: &[u8]) -> IResult<&[u8], &[u8]> {
    let (res, (_, v, _)) = tuple((tag(&b"\n"[..]), parse_value_binary_int, tag(&b"\n"[..])))(s)?;
    Ok((res, v))
}
fn parse_value_binary_int(s: &[u8]) -> IResult<&[u8], &[u8]> {
    let (s, u) = le_u64(s)?;
    take(u as usize)(s)
}
fn parse_value_string(s: &[u8]) -> IResult<&[u8], &[u8]> {
    let (res, (_, v, _)) = tuple((
        tag(&b"="[..]),
        opt(take_while1(|c| c != NEWLINE)),
        tag(&b"\n"[..]),
    ))(s)?;
    Ok((res, v.unwrap_or(&b""[..])))
}

#[test]
fn parse_value_test() {
    // String values
    assert_eq!(parse_value(b"=123\n"), Ok((&b""[..], &b"123"[..])));
    assert_eq!(
        parse_value(b"=latin"),
        Err(Err::Incomplete(Needed::Size(1)))
    );

    // Binary values
    assert_eq!(
        parse_value(&[
            0x0A, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x66, 0x6f, 0x6f, 0x0a, 0x62,
            0x61, 0x72, 0x0a, 0x66, 0x6f, 0x6f
        ]),
        Ok((&b"foo"[..], &b"foo\nbar"[..]))
    );
    assert_eq!(
        parse_value(&[0x0A, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x66]),
        Err(Err::Incomplete(Needed::Size(7)))
    );
    assert_eq!(
        parse_value(&[0x0A, 0x07]),
        Err(Err::Incomplete(Needed::Size(8)))
    );
}

use nom::combinator::opt;
use nom::sequence::pair;

fn parse_key_value(s: &[u8]) -> IResult<&[u8], KVP> {
    pair(parse_key, parse_value)(s)
}

#[test]
fn parse_key_value_test() {
    assert_eq!(
        parse_key_value(b"uid=1\n123"),
        Ok((&b"123"[..], (&b"uid"[..], &b"1"[..])))
    );
    assert_eq!(
        parse_key_value(b"uid"),
        Err(Err::Incomplete(Needed::Size(1)))
    );
    assert_eq!(
        parse_key_value(b"uid="),
        Err(Err::Incomplete(Needed::Size(1)))
    );
}

type KVP <'a> = (&'a [u8], &'a [u8]);

fn parse_end_of_msg(s: &[u8]) -> IResult<&[u8], Option<KVP>> {
    let newline: [u8; 1] = [NEWLINE];

    // if the character we're reading is a newline, it means we're at a message separator, so we return none
    // if we get a tag error, it's not a new message, so fallthrough and parse the key_value
    // on any other errors, return the error (such as EOF or need more data, etc)
    match tag(newline)(s) {
        Ok(_) => {
            //eprintln!("Read end of message newline");
            return Ok((&s[1..], None));
        }
        Err(e) => {
            match e {
                Err::Error((_, ErrorKind::Tag)) => (), /* if we didn't match the tag, fall through */
                _ => return Err(e),
            }
        }
    };

    match parse_key_value(s) {
        Ok((input, res)) => Ok((input, Some(res))),
        Err(e) => Err(e),
    }
}

#[test]
fn parse_end_of_message_test() {
    assert_eq!(
        parse_end_of_msg(b"uid=1\n123"),
        Ok((&b"123"[..], Some((&b"uid"[..], &b"1"[..]))))
    );
    assert_eq!(parse_end_of_msg(b""), Err(Err::Incomplete(Needed::Size(1))));
    assert_eq!(parse_end_of_msg(b"\n"), Ok((&b""[..], None)));
    assert_eq!(parse_end_of_msg(b"\nu"), Ok((&b"u"[..], None)));
}

use flate2::read::GzDecoder;
use std::fs::File;
use std::io::prelude::*;

#[derive(Debug, PartialEq, Clone)]
pub struct JournalMessage {
    fields: Vec<(Vec<u8>, Vec<u8>)>,
}

use chrono::{DateTime, NaiveDateTime, Utc};

// Well known fields: https://www.freedesktop.org/software/systemd/man/systemd.journal-fields.html
impl<'a> JournalMessage {
    pub fn to_string(&self, mode: Option<OutputMode>) -> String {
        match mode {
            None => format!(
                "{} {} {}[{}]: {}\n",
                self.timestamp().unwrap_or_else(|| "".to_owned()),
                self.hostname(),
                self.comm(),
                self.pid(),
                self.message().unwrap_or_else(|| "".to_owned()),
            ),
            Some(mode) => match mode {
                OutputMode::short_iso => format!(
                    "{} {} {}[{}]: {}\n",
                    self.timestamp().unwrap_or_else(|| "".to_owned()),
                    self.hostname(),
                    self.comm(),
                    self.pid(),
                    self.message().unwrap_or_else(|| "".to_owned()),
                ),
                _ => panic!("output mode '{}' not implemented", mode),
            },
        }
    }

    pub fn message(&self) -> Option<String> {
        let key = b"MESSAGE";
        match self.field(key) {
            Some(s) => {
                // Sometimes message might be empty, if it is
                // try and return SYSLOG_RAW instead
                if s.is_empty() {
                    let syslog_raw = b"SYSLOG_RAW";
                    self.field(syslog_raw)
                } else {
                    Some(s)
                }
            }
            None => None,
        }
    }

    pub fn pid(&self) -> String {
        let key = b"_PID";
        self.field(key).unwrap_or_else(|| "".to_owned())
    }

    /*
    pub fn uid(&self) -> String {
        let key = b"_UID";
        self.field(key).unwrap_or_else(|| "".to_owned())
    }
    */

    /*
    pub fn gid(&self) -> String {
        let key = b"_GID";
        self.field(key).unwrap_or_else(|| "".to_owned())
    }
    */

    pub fn comm(&self) -> String {
        let key = b"_COMM";
        self.field(key).unwrap_or_else(|| "".to_owned())
    }

    /*
    pub fn exe(&self) -> String {
        let key = b"_EXE";
        self.field(key).unwrap_or_else(|| "".to_owned())
    }
    */

    /*
    pub fn cmdline(&self) -> String {
        let key = b"_CMDLINE";
        self.field(key).unwrap_or_else(|| "".to_owned())
    }
    */

    /*
    pub fn systemd_cgroup(&self) -> String {
        let key = b"_SYSTEMD_CGROUP";
        self.field(key).unwrap_or_else(|| "".to_owned())
    }
    */

    /*
    pub fn systemd_slice(&self) -> String {
        let key = b"_SYSTEMD_SLICE";
        self.field(key).unwrap_or_else(|| "".to_owned())
    }
    */

    pub fn systemd_unit(&self) -> String {
        let key = b"_SYSTEMD_UNIT";
        self.field(key).unwrap_or_else(|| "".to_owned())
    }

    pub fn hostname(&self) -> String {
        let key = b"_HOSTNAME";
        self.field(key).unwrap_or_else(|| "".to_owned())
    }

    /*
    pub fn machine_id(&self) -> String {
        let key = b"_MACHINE_ID";
        self.field(key).unwrap_or_else(|| "".to_owned())
    }
    */

    pub fn timestamp(&self) -> Option<String> {
        if let Some(date) = self.date_time() {
            return Some(date.format("%+").to_string())
        }

        None
    }

    fn date_time(&self) -> Option<DateTime<Utc>> {
        let key = b"_SOURCE_REALTIME_TIMESTAMP";
        let key2 = b"__REALTIME_TIMESTAMP";
        let s = match self.field(key) {
            Some(s) => s,
            None => match self.field(key2) {
                Some(s) => s,
                None => return None,
            },
        };

        //eprintln!("timestamp: {}", s);
        let micros = match s.parse::<i64>() {
            Ok(n) => n,
            Err(e) => {
                eprintln!("Error parsing string to i64 {}: {:?}", s, e);
                return None;
            }
        };

        // convert from microseconds to seconds and nanoseconds for date lib
        let secs = micros / 1_000_000;
        let nanos = micros - (secs * 1_000_000);

        let ts = NaiveDateTime::from_timestamp(secs, nanos as u32);
        let ts_utc: DateTime<Utc> = DateTime::from_utc(ts, Utc);

        Some(ts_utc)
    }

    pub fn field(&self, key: &[u8]) -> Option<String> {
        for (k, v) in &self.fields {
            if Vec::from(key) == *k {
                return Some(std::str::from_utf8(&v[..]).unwrap().to_owned());
            }
        }

        None
    }
}

use structopt::clap::arg_enum;
use structopt::StructOpt;

arg_enum! {
    /*
      -o --output=STRING         Change journal output mode (short, short-precise,
                               short-iso, short-iso-precise, short-full,
                               short-monotonic, short-unix, verbose, export,
                               json, json-pretty, json-sse, json-seq, cat,
                               with-unit)
    */
    #[derive(Debug, Clone)]
    #[allow(non_camel_case_types)]
    pub enum OutputMode {
        short,
        short_precise,
        short_iso,
        short_iso_precise,
        short_full,
        short_monotonic,
        short_unix,
        verbose,
        export,
        json,
        json_pretty,
        json_sse,
        json_seq,
        cat,
        with_unit,
    }
}

#[derive(StructOpt, Debug, Clone)]
pub struct Filter {
    // Show entries starting at the specified cursor
    //#[structopt(short, long)]
    //cursor: Option<String>,

    // Print the cursor after all the entries
    //#[structopt(long)]
    //show_cursor: bool,

    // Show entries after the specified cursor
    //#[structopt(long)]
    //after_cursor: Option<String>,

    /// Show logs from the specified unit
    #[structopt(short, long)]
    unit: Option<String>,

    /// Show entries not older than the specified date
    #[structopt(short = "S", long, parse(try_from_str = parse_rel_time))]
    since: Option<DateTime<chrono::Local>>,

    /// Show entries not newer than the specified date
    #[structopt(short = "U", long, parse(try_from_str = parse_rel_time))]
    until: Option<DateTime<chrono::Local>>,

    /// Number of journal entries to show
    #[structopt(short = "n", long)]
    pub lines: Option<u64>,

    

    // Suppress output of hostname field
    //#[structopt(long)]
    //no_hostname: bool,
}

use chrono::prelude::*;
use chrono_english::{parse_date_string, DateResult, Dialect};

//fn parse_rel_time<T, U>(s: &str) -> Result<(T, U), Box<dyn Error>>
fn parse_rel_time(s: &str) -> DateResult<DateTime<chrono::Local>> {
    parse_date_string(s, Local::now(), Dialect::Us)
}

pub struct JournalBackupReader {
    reader: Box<dyn ::std::io::Read>,
    remainder: Vec<u8>,
    remainder_read: usize,

    filter: Option<Filter>,
}

impl JournalBackupReader {
    pub fn new(reader: Box<dyn ::std::io::Read>, filter: Option<Filter>) -> JournalBackupReader {
        JournalBackupReader {
            reader,
            filter,
            remainder: Vec::new(),
            remainder_read: 0,
        }
    }

    pub fn open_file(file: String, filter: Option<Filter>) -> std::io::Result<JournalBackupReader> {
        let mut file = File::open(file)?;

        let mut buffer = [0u8; 2];

        file.read_exact(&mut buffer)?;
        file.seek(std::io::SeekFrom::Start(0))?;

        if is_gz_magic(&buffer[..]) {
            Ok(JournalBackupReader::new(
                Box::new(GzDecoder::new(file)),
                filter,
            ))
        } else {
            Ok(JournalBackupReader::new(Box::new(file), filter))
        }
    }

    fn read(&mut self) -> Option<usize> {
        let new_vec = Vec::from(&self.remainder[self.remainder_read..]);
        self.remainder = new_vec;
        self.remainder_read = 0;

        let mut buffer = [0; 32_768];
        match self.reader.read(&mut buffer) {
            Ok(l) => {
                self.remainder.extend_from_slice(&buffer[..l]);
                return Some(l);
            }
            Err(e) => eprintln!("read error: {:?}", e),
        }

        None
    }

    fn should_filter(&mut self, msg: &JournalMessage) -> bool {
        match &self.filter {
            Some(filter) => {
                let mut should_filter = false;

                if let Some(unit) = &filter.unit {
                    if *unit != msg.systemd_unit() {
                        //eprintln!("{:?} != {:?}", unit, msg.systemd_unit().unwrap());
                        should_filter = true;
                    }
                }

                if let Some(filter_since) = &filter.since {
                    if let Some(time) = msg.date_time() {
                        if time < *filter_since {
                            should_filter = true;
                        }
                    }
                }

                if let Some(filter_until) = &filter.until {
                    if let Some(time) = msg.date_time() {
                        if time > *filter_until {
                            should_filter = true;
                        }
                    }
                }

                should_filter
            }
            None => false,
        }
    }
}

impl Iterator for JournalBackupReader {
    type Item = JournalMessage;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remainder.is_empty() {
            match self.read() {
                Some(l) => {
                    if let 0 = l {
                        return None;
                    }
                } // EOF return None
                None => return None,
            }
        }

        let mut result = JournalMessage { fields: Vec::new() };

        // if we've read in more than 10MiB something is probably wrong and we should quit processing
        while self.remainder.len() < 10_000_000 {
            let mut more = false;

            match parse_end_of_msg(&self.remainder[self.remainder_read..]) {
                // TODO: no clone
                Ok((rem, kvp)) => {
                    //self.remainder = rem.to_vec();
                    self.remainder_read = self.remainder.len() - rem.len();
                    match kvp {
                        Some((key, value)) => {
                            /*eprintln!(
                                "kvp: {} = {}",
                                std::str::from_utf8(key).unwrap(),
                                std::str::from_utf8(value).unwrap()
                            );*/
                            result.fields.push((key.to_vec(), value.to_vec()));
                        }
                        None => {
                            if !self.should_filter(&result) {
                                return Some(result);
                            } else {
                                result = JournalMessage { fields: Vec::new() };
                            }
                        }
                    }
                }
                Err(e) => match e {
                    Err::Incomplete(_) => {
                        more = true;
                    }
                    Err::Error((_, kind)) => panic!("Unexpected parser error: {:?}", kind),
                    Err::Failure(e) => panic!("Unexpected parser error: {:?}", e),
                },
            }

            if more {
                match self.read() {
                    Some(l) => {
                        if let 0 = l {
                            return None;
                        }
                    } // EOF return None
                    None => return None,
                }
            }
        }

        panic!("Runaway memory growth in journal parsing")
    }
}

#[test]
fn full_message_test() {
    let data = include_bytes!("../assets/journal.binary.example");
    println!("data: {}", data.len());
    let mut r = JournalBackupReader::new(Box::new(&data[..]), None);

    assert_eq!(r.next(), Some(JournalMessage{
        fields: vec!(
            (b"__CURSOR".to_vec(), b"s=01a7f10c068a4cbea936aac77adaaa98;i=ee9b5;b=0c7ce331b7e844cba8d33586d7903e8a;m=11090311;t=5ad95a03664c9;x=cdabffa65cc76271".to_vec()),
                (b"__REALTIME_TIMESTAMP".to_vec(), b"1598233033204937".to_vec()),
                (b"__MONOTONIC_TIMESTAMP".to_vec(), b"285803281".to_vec()),
                (b"_BOOT_ID".to_vec(), b"0c7ce331b7e844cba8d33586d7903e8a".to_vec()),
                (b"_TRANSPORT".to_vec(), b"journal".to_vec()),
                (b"_UID".to_vec(), b"1003".to_vec()),
                (b"_GID".to_vec(), b"1005".to_vec()),
                (b"_CAP_EFFECTIVE".to_vec(), b"0".to_vec()),
                (b"_SELINUX_CONTEXT".to_vec(), b"unconfined\n".to_vec()),
                (b"_AUDIT_LOGINUID".to_vec(), b"1003".to_vec()),
                (b"_SYSTEMD_OWNER_UID".to_vec(), b"1003".to_vec()),
                (b"_SYSTEMD_SLICE".to_vec(), b"user-1003.slice".to_vec()),
                (b"_SYSTEMD_USER_SLICE".to_vec(), b"-.slice".to_vec()),
                (b"_MACHINE_ID".to_vec(), b"95d084728d146225df1ecebe941dc596".to_vec()),
                (b"_HOSTNAME".to_vec(), b"knisbet-dev".to_vec()),
                (b"MESSAGE".to_vec(), b"foo\nbar".to_vec()),
                (b"CODE_FILE".to_vec(), b"<string>".to_vec()),
                (b"CODE_LINE".to_vec(), b"1".to_vec()),
                (b"CODE_FUNC".to_vec(), b"<module>".to_vec()),
                (b"SYSLOG_IDENTIFIER".to_vec(), b"python3".to_vec()),
                (b"_COMM".to_vec(), b"python3".to_vec()),
                (b"_EXE".to_vec(), b"/usr/bin/python3.7".to_vec()),
                (b"_CMDLINE".to_vec(), b"python3 -c from systemd import journal; journal.send(\"foo\\nbar\")".to_vec()),
                (b"_AUDIT_SESSION".to_vec(), b"1".to_vec()),
                (b"_SYSTEMD_CGROUP".to_vec(), b"/user.slice/user-1003.slice/session-1.scope".to_vec()),
                (b"_SYSTEMD_SESSION".to_vec(), b"1".to_vec()),
                (b"_SYSTEMD_UNIT".to_vec(), b"session-1.scope".to_vec()),
                (b"_SYSTEMD_INVOCATION_ID".to_vec(), b"b63da6c195c04def8c059b2323b8a179".to_vec()),
                (b"_PID".to_vec(), b"2331".to_vec()),
                (b"_SOURCE_REALTIME_TIMESTAMP".to_vec(), b"1598233033204859".to_vec()),
        )
    }));
    assert_eq!(r.next(), None);
}

#[test]
fn multiple_message_test() {
    let data = include_bytes!("../assets/journal.export.3.example");
    let mut r = JournalBackupReader::new(Box::new(&data[..]), None);

    // TODO: table test?
    let compressed = include_bytes!("../assets/journal.export.3.example.gz");
    let mut r2 = JournalBackupReader::new(Box::new(GzDecoder::new(&compressed[..])), None);

    // helper to generate match
    // cat assets/journal.export.3.example | awk '{sub(/=/,"|")}1'  | awk -F'|' '{printf "%s%s%s%s%s\n", "(b\"", $1, "\".to_vec(), b\"", $2, "\".to_vec()),"}'
    assert_eq!(r.next(), Some(JournalMessage{
        fields: vec!(
            (b"__CURSOR".to_vec(), b"s=4d4c07169cf346bf84c0682dee9f876d;i=f7101;b=29afc66917be48d58ba2a628b946422c;m=a2531317;t=5ae0622d0fbb8;x=966282a14870533f".to_vec()),
(b"__REALTIME_TIMESTAMP".to_vec(), b"1598716260711352".to_vec()),
(b"__MONOTONIC_TIMESTAMP".to_vec(), b"2723353367".to_vec()),
(b"_BOOT_ID".to_vec(), b"29afc66917be48d58ba2a628b946422c".to_vec()),
(b"_TRANSPORT".to_vec(), b"syslog".to_vec()),
(b"_SELINUX_CONTEXT".to_vec(), b"unconfined\n".to_vec()),
(b"_SYSTEMD_SLICE".to_vec(), b"system.slice".to_vec()),
(b"_MACHINE_ID".to_vec(), b"95d084728d146225df1ecebe941dc596".to_vec()),
(b"_HOSTNAME".to_vec(), b"knisbet-dev".to_vec()),
(b"PRIORITY".to_vec(), b"4".to_vec()),
(b"_CAP_EFFECTIVE".to_vec(), b"0".to_vec()),
(b"SYSLOG_FACILITY".to_vec(), b"5".to_vec()),
(b"SYSLOG_IDENTIFIER".to_vec(), b"rsyslogd".to_vec()),
(b"_UID".to_vec(), b"104".to_vec()),
(b"_GID".to_vec(), b"109".to_vec()),
(b"_COMM".to_vec(), b"rsyslogd".to_vec()),
(b"_EXE".to_vec(), b"/usr/sbin/rsyslogd".to_vec()),
(b"_CMDLINE".to_vec(), b"/usr/sbin/rsyslogd -n -iNONE".to_vec()),
(b"_SYSTEMD_CGROUP".to_vec(), b"/system.slice/rsyslog.service".to_vec()),
(b"_SYSTEMD_UNIT".to_vec(), b"rsyslog.service".to_vec()),
(b"MESSAGE".to_vec(), b"action 'action-8-builtin:omfile' suspended (module 'builtin:omfile'), retry 0. There should be messages before this one giving the reason for suspension. [v8.1901.0 try https://www.rsyslog.com/e/2007 ]".to_vec()),
(b"_PID".to_vec(), b"654".to_vec()),
(b"_SYSTEMD_INVOCATION_ID".to_vec(), b"c07138f522f44a6399a14c99d83c8313".to_vec()),
(b"SYSLOG_TIMESTAMP".to_vec(), b"Aug 29 15:51:00 ".to_vec()),
(b"_SOURCE_REALTIME_TIMESTAMP".to_vec(), b"1598716260706706".to_vec()),
        )
    }));

    assert_eq!(r2.next(), Some(JournalMessage{
        fields: vec!(
            (b"__CURSOR".to_vec(), b"s=4d4c07169cf346bf84c0682dee9f876d;i=f7101;b=29afc66917be48d58ba2a628b946422c;m=a2531317;t=5ae0622d0fbb8;x=966282a14870533f".to_vec()),
(b"__REALTIME_TIMESTAMP".to_vec(), b"1598716260711352".to_vec()),
(b"__MONOTONIC_TIMESTAMP".to_vec(), b"2723353367".to_vec()),
(b"_BOOT_ID".to_vec(), b"29afc66917be48d58ba2a628b946422c".to_vec()),
(b"_TRANSPORT".to_vec(), b"syslog".to_vec()),
(b"_SELINUX_CONTEXT".to_vec(), b"unconfined\n".to_vec()),
(b"_SYSTEMD_SLICE".to_vec(), b"system.slice".to_vec()),
(b"_MACHINE_ID".to_vec(), b"95d084728d146225df1ecebe941dc596".to_vec()),
(b"_HOSTNAME".to_vec(), b"knisbet-dev".to_vec()),
(b"PRIORITY".to_vec(), b"4".to_vec()),
(b"_CAP_EFFECTIVE".to_vec(), b"0".to_vec()),
(b"SYSLOG_FACILITY".to_vec(), b"5".to_vec()),
(b"SYSLOG_IDENTIFIER".to_vec(), b"rsyslogd".to_vec()),
(b"_UID".to_vec(), b"104".to_vec()),
(b"_GID".to_vec(), b"109".to_vec()),
(b"_COMM".to_vec(), b"rsyslogd".to_vec()),
(b"_EXE".to_vec(), b"/usr/sbin/rsyslogd".to_vec()),
(b"_CMDLINE".to_vec(), b"/usr/sbin/rsyslogd -n -iNONE".to_vec()),
(b"_SYSTEMD_CGROUP".to_vec(), b"/system.slice/rsyslog.service".to_vec()),
(b"_SYSTEMD_UNIT".to_vec(), b"rsyslog.service".to_vec()),
(b"MESSAGE".to_vec(), b"action 'action-8-builtin:omfile' suspended (module 'builtin:omfile'), retry 0. There should be messages before this one giving the reason for suspension. [v8.1901.0 try https://www.rsyslog.com/e/2007 ]".to_vec()),
(b"_PID".to_vec(), b"654".to_vec()),
(b"_SYSTEMD_INVOCATION_ID".to_vec(), b"c07138f522f44a6399a14c99d83c8313".to_vec()),
(b"SYSLOG_TIMESTAMP".to_vec(), b"Aug 29 15:51:00 ".to_vec()),
(b"_SOURCE_REALTIME_TIMESTAMP".to_vec(), b"1598716260706706".to_vec()),
        )
    }));

    assert_eq!(r.next(), Some(JournalMessage{
        fields: vec!(
            (b"__CURSOR".to_vec(), b"s=4d4c07169cf346bf84c0682dee9f876d;i=f7102;b=29afc66917be48d58ba2a628b946422c;m=a253133c;t=5ae0622d0fbdd;x=334ef41b13d414a9".to_vec()),
(b"__REALTIME_TIMESTAMP".to_vec(), b"1598716260711389".to_vec()),
(b"__MONOTONIC_TIMESTAMP".to_vec(), b"2723353404".to_vec()),
(b"_BOOT_ID".to_vec(), b"29afc66917be48d58ba2a628b946422c".to_vec()),
(b"_TRANSPORT".to_vec(), b"syslog".to_vec()),
(b"_SELINUX_CONTEXT".to_vec(), b"unconfined\n".to_vec()),
(b"_SYSTEMD_SLICE".to_vec(), b"system.slice".to_vec()),
(b"_MACHINE_ID".to_vec(), b"95d084728d146225df1ecebe941dc596".to_vec()),
(b"_HOSTNAME".to_vec(), b"knisbet-dev".to_vec()),
(b"PRIORITY".to_vec(), b"4".to_vec()),
(b"_CAP_EFFECTIVE".to_vec(), b"0".to_vec()),
(b"SYSLOG_FACILITY".to_vec(), b"5".to_vec()),
(b"SYSLOG_IDENTIFIER".to_vec(), b"rsyslogd".to_vec()),
(b"_UID".to_vec(), b"104".to_vec()),
(b"_GID".to_vec(), b"109".to_vec()),
(b"_COMM".to_vec(), b"rsyslogd".to_vec()),
(b"_EXE".to_vec(), b"/usr/sbin/rsyslogd".to_vec()),
(b"_CMDLINE".to_vec(), b"/usr/sbin/rsyslogd -n -iNONE".to_vec()),
(b"_SYSTEMD_CGROUP".to_vec(), b"/system.slice/rsyslog.service".to_vec()),
(b"_SYSTEMD_UNIT".to_vec(), b"rsyslog.service".to_vec()),
(b"_PID".to_vec(), b"654".to_vec()),
(b"_SYSTEMD_INVOCATION_ID".to_vec(), b"c07138f522f44a6399a14c99d83c8313".to_vec()),
(b"SYSLOG_TIMESTAMP".to_vec(), b"Aug 29 15:51:00 ".to_vec()),
(b"MESSAGE".to_vec(), b"action 'action-8-builtin:omfile' suspended (module 'builtin:omfile'), next retry is Sat Aug 29 15:51:30 2020, retry nbr 0. There should be messages before this one giving the reason for suspension. [v8.1901.0 try https://www.rsyslog.com/e/2007 ]".to_vec()),
(b"_SOURCE_REALTIME_TIMESTAMP".to_vec(), b"1598716260706709".to_vec()),
        )
    }));

    assert_eq!(r2.next(), Some(JournalMessage{
        fields: vec!(
            (b"__CURSOR".to_vec(), b"s=4d4c07169cf346bf84c0682dee9f876d;i=f7102;b=29afc66917be48d58ba2a628b946422c;m=a253133c;t=5ae0622d0fbdd;x=334ef41b13d414a9".to_vec()),
(b"__REALTIME_TIMESTAMP".to_vec(), b"1598716260711389".to_vec()),
(b"__MONOTONIC_TIMESTAMP".to_vec(), b"2723353404".to_vec()),
(b"_BOOT_ID".to_vec(), b"29afc66917be48d58ba2a628b946422c".to_vec()),
(b"_TRANSPORT".to_vec(), b"syslog".to_vec()),
(b"_SELINUX_CONTEXT".to_vec(), b"unconfined\n".to_vec()),
(b"_SYSTEMD_SLICE".to_vec(), b"system.slice".to_vec()),
(b"_MACHINE_ID".to_vec(), b"95d084728d146225df1ecebe941dc596".to_vec()),
(b"_HOSTNAME".to_vec(), b"knisbet-dev".to_vec()),
(b"PRIORITY".to_vec(), b"4".to_vec()),
(b"_CAP_EFFECTIVE".to_vec(), b"0".to_vec()),
(b"SYSLOG_FACILITY".to_vec(), b"5".to_vec()),
(b"SYSLOG_IDENTIFIER".to_vec(), b"rsyslogd".to_vec()),
(b"_UID".to_vec(), b"104".to_vec()),
(b"_GID".to_vec(), b"109".to_vec()),
(b"_COMM".to_vec(), b"rsyslogd".to_vec()),
(b"_EXE".to_vec(), b"/usr/sbin/rsyslogd".to_vec()),
(b"_CMDLINE".to_vec(), b"/usr/sbin/rsyslogd -n -iNONE".to_vec()),
(b"_SYSTEMD_CGROUP".to_vec(), b"/system.slice/rsyslog.service".to_vec()),
(b"_SYSTEMD_UNIT".to_vec(), b"rsyslog.service".to_vec()),
(b"_PID".to_vec(), b"654".to_vec()),
(b"_SYSTEMD_INVOCATION_ID".to_vec(), b"c07138f522f44a6399a14c99d83c8313".to_vec()),
(b"SYSLOG_TIMESTAMP".to_vec(), b"Aug 29 15:51:00 ".to_vec()),
(b"MESSAGE".to_vec(), b"action 'action-8-builtin:omfile' suspended (module 'builtin:omfile'), next retry is Sat Aug 29 15:51:30 2020, retry nbr 0. There should be messages before this one giving the reason for suspension. [v8.1901.0 try https://www.rsyslog.com/e/2007 ]".to_vec()),
(b"_SOURCE_REALTIME_TIMESTAMP".to_vec(), b"1598716260706709".to_vec()),
        )
    }));

    assert_eq!(r.next(), Some(JournalMessage{
        fields: vec!(
            (b"__CURSOR".to_vec(), b"s=4d4c07169cf346bf84c0682dee9f876d;i=f7103;b=29afc66917be48d58ba2a628b946422c;m=a2537bc6;t=5ae0622d16466;x=6172411011d0f348".to_vec()),
(b"__REALTIME_TIMESTAMP".to_vec(), b"1598716260738150".to_vec()),
(b"__MONOTONIC_TIMESTAMP".to_vec(), b"2723380166".to_vec()),
(b"_BOOT_ID".to_vec(), b"29afc66917be48d58ba2a628b946422c".to_vec()),
(b"SYSLOG_FACILITY".to_vec(), b"3".to_vec()),
(b"_SELINUX_CONTEXT".to_vec(), b"unconfined\n".to_vec()),
(b"_SYSTEMD_SLICE".to_vec(), b"system.slice".to_vec()),
(b"_MACHINE_ID".to_vec(), b"95d084728d146225df1ecebe941dc596".to_vec()),
(b"_HOSTNAME".to_vec(), b"knisbet-dev".to_vec()),
(b"_TRANSPORT".to_vec(), b"journal".to_vec()),
(b"PRIORITY".to_vec(), b"4".to_vec()),
(b"CODE_FILE".to_vec(), b"../src/resolve/resolved-dns-transaction.c".to_vec()),
(b"CODE_LINE".to_vec(), b"1049".to_vec()),
(b"CODE_FUNC".to_vec(), b"dns_transaction_process_reply".to_vec()),
(b"SYSLOG_IDENTIFIER".to_vec(), b"systemd-resolved".to_vec()),
(b"MESSAGE".to_vec(), b"Server returned error NXDOMAIN, mitigating potential DNS violation DVE-2018-0001, retrying transaction with reduced feature level UDP.".to_vec()),
(b"_UID".to_vec(), b"102".to_vec()),
(b"_GID".to_vec(), b"104".to_vec()),
(b"_COMM".to_vec(), b"systemd-resolve".to_vec()),
(b"_EXE".to_vec(), b"/usr/lib/systemd/systemd-resolved".to_vec()),
(b"_CMDLINE".to_vec(), b"/lib/systemd/systemd-resolved".to_vec()),
(b"_CAP_EFFECTIVE".to_vec(), b"0".to_vec()),
(b"_SYSTEMD_CGROUP".to_vec(), b"/system.slice/systemd-resolved.service".to_vec()),
(b"_SYSTEMD_UNIT".to_vec(), b"systemd-resolved.service".to_vec()),
(b"_PID".to_vec(), b"590".to_vec()),
(b"_SYSTEMD_INVOCATION_ID".to_vec(), b"199c71154e9d44e7acdaeaa87a0e5a7e".to_vec()),
(b"_SOURCE_REALTIME_TIMESTAMP".to_vec(), b"1598716260738112".to_vec()),
        )
    }));

    assert_eq!(r2.next(), Some(JournalMessage{
        fields: vec!(
            (b"__CURSOR".to_vec(), b"s=4d4c07169cf346bf84c0682dee9f876d;i=f7103;b=29afc66917be48d58ba2a628b946422c;m=a2537bc6;t=5ae0622d16466;x=6172411011d0f348".to_vec()),
(b"__REALTIME_TIMESTAMP".to_vec(), b"1598716260738150".to_vec()),
(b"__MONOTONIC_TIMESTAMP".to_vec(), b"2723380166".to_vec()),
(b"_BOOT_ID".to_vec(), b"29afc66917be48d58ba2a628b946422c".to_vec()),
(b"SYSLOG_FACILITY".to_vec(), b"3".to_vec()),
(b"_SELINUX_CONTEXT".to_vec(), b"unconfined\n".to_vec()),
(b"_SYSTEMD_SLICE".to_vec(), b"system.slice".to_vec()),
(b"_MACHINE_ID".to_vec(), b"95d084728d146225df1ecebe941dc596".to_vec()),
(b"_HOSTNAME".to_vec(), b"knisbet-dev".to_vec()),
(b"_TRANSPORT".to_vec(), b"journal".to_vec()),
(b"PRIORITY".to_vec(), b"4".to_vec()),
(b"CODE_FILE".to_vec(), b"../src/resolve/resolved-dns-transaction.c".to_vec()),
(b"CODE_LINE".to_vec(), b"1049".to_vec()),
(b"CODE_FUNC".to_vec(), b"dns_transaction_process_reply".to_vec()),
(b"SYSLOG_IDENTIFIER".to_vec(), b"systemd-resolved".to_vec()),
(b"MESSAGE".to_vec(), b"Server returned error NXDOMAIN, mitigating potential DNS violation DVE-2018-0001, retrying transaction with reduced feature level UDP.".to_vec()),
(b"_UID".to_vec(), b"102".to_vec()),
(b"_GID".to_vec(), b"104".to_vec()),
(b"_COMM".to_vec(), b"systemd-resolve".to_vec()),
(b"_EXE".to_vec(), b"/usr/lib/systemd/systemd-resolved".to_vec()),
(b"_CMDLINE".to_vec(), b"/lib/systemd/systemd-resolved".to_vec()),
(b"_CAP_EFFECTIVE".to_vec(), b"0".to_vec()),
(b"_SYSTEMD_CGROUP".to_vec(), b"/system.slice/systemd-resolved.service".to_vec()),
(b"_SYSTEMD_UNIT".to_vec(), b"systemd-resolved.service".to_vec()),
(b"_PID".to_vec(), b"590".to_vec()),
(b"_SYSTEMD_INVOCATION_ID".to_vec(), b"199c71154e9d44e7acdaeaa87a0e5a7e".to_vec()),
(b"_SOURCE_REALTIME_TIMESTAMP".to_vec(), b"1598716260738112".to_vec()),
        )
    }));

    assert_eq!(r.next(), None);
    assert_eq!(r2.next(), None);
}

fn is_gz_magic(s: &[u8]) -> bool {
    fn gz_magic(s: &[u8]) -> IResult<&[u8], &[u8]> {
        let gz_magic: &[u8] = &[0x1f, 0x8b];
        tag(gz_magic)(s)
    }

    matches!(gz_magic(s), Ok(_))
}

