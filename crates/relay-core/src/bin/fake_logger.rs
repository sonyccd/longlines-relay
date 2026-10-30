//! Dev tool: sends N1MM-format contact packets to the relay on this computer.
//!
//! cargo run -p relay-core --bin fake-logger -- --port 12060 --mycall KK4PWJ --call W1AW \
//!     --sequence info,replace,delete --offset-minutes 0

use std::process::ExitCode;
use std::time::Duration;

use chrono::{TimeDelta, Utc};
use relay_core::fake_logger::{radioinfo_xml, send, FakeContact, MALFORMED};
use relay_core::settings::DEFAULT_PORT;

const USAGE: &str = "\
fake-logger: send N1MM-format packets to 127.0.0.1

Options:
  --port N             UDP port (default 12060)
  --mycall CALL        station callsign (default KK4PWJ; \"\" sends an empty one)
  --call CALL          worked station (default W1AW)
  --freq N             rxfreq/txfreq in 10 Hz units (default 1402500, 14.025 MHz)
  --mode MODE          mode (default CW)
  --app NAME           app element (default N1MM)
  --grid GRID          gridsquare element, the worked station's locator (default empty)
  --section TEXT       section element (default empty)
  --qth TEXT           qth element (default empty)
  --exchange1 TEXT     exchange1 element (default empty)
  --sequence LIST      comma-separated: info, replace, delete, radioinfo, malformed (default info)
  --offset-minutes N   timestamp N minutes before now; negative is in the future (default 0)
  --id ID              contact ID (default a fresh 32-hex ID)
  --no-id              leave the ID element out
  --forwarded          IsOriginal False (a contact forwarded by another N1MM station)
  --malformed          also send a malformed packet at the end
  --delay-ms N         pause between packets (default 300)
  --help               this text

replace keeps the ID and moves the frequency up 1 kHz, as an edit would.";

struct Args {
    port: u16,
    contact: FakeContact,
    sequence: Vec<String>,
    malformed: bool,
    delay: Duration,
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut args = std::env::args().skip(1);
    let mut port = DEFAULT_PORT;
    let mut contact = FakeContact::new("KK4PWJ", "W1AW", Utc::now());
    let mut offset_minutes: i64 = 0;
    let mut sequence = vec!["info".to_string()];
    let mut malformed = false;
    let mut delay = Duration::from_millis(300);
    while let Some(flag) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match flag.as_str() {
            "--help" | "-h" => return Ok(None),
            "--port" => {
                port = value("--port")?
                    .parse()
                    .map_err(|e| format!("--port: {e}"))?
            }
            "--mycall" => contact.mycall = value("--mycall")?,
            "--call" => contact.call = value("--call")?,
            "--freq" => {
                contact.freq_10hz = value("--freq")?
                    .parse()
                    .map_err(|e| format!("--freq: {e}"))?
            }
            "--mode" => contact.mode = value("--mode")?,
            "--app" => contact.app = value("--app")?,
            "--grid" => contact.gridsquare = value("--grid")?,
            "--section" => contact.section = value("--section")?,
            "--qth" => contact.qth = value("--qth")?,
            "--exchange1" => contact.exchange1 = value("--exchange1")?,
            "--sequence" => {
                sequence = value("--sequence")?
                    .split(',')
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "--offset-minutes" => {
                offset_minutes = value("--offset-minutes")?
                    .parse()
                    .map_err(|e| format!("--offset-minutes: {e}"))?;
            }
            "--id" => contact.id = Some(value("--id")?),
            "--no-id" => contact.id = None,
            "--forwarded" => contact.is_original = false,
            "--malformed" => malformed = true,
            "--delay-ms" => {
                delay = Duration::from_millis(
                    value("--delay-ms")?
                        .parse()
                        .map_err(|e| format!("--delay-ms: {e}"))?,
                );
            }
            other => return Err(format!("unknown option {other}")),
        }
    }
    contact.timestamp = Utc::now() - TimeDelta::minutes(offset_minutes);
    for step in &sequence {
        if !["info", "replace", "delete", "radioinfo", "malformed"].contains(&step.as_str()) {
            return Err(format!("unknown sequence step {step}"));
        }
    }
    Ok(Some(Args {
        port,
        contact,
        sequence,
        malformed,
        delay,
    }))
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(a)) => a,
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("fake-logger: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let mut steps = args.sequence.clone();
    if args.malformed {
        steps.push("malformed".into());
    }
    let mut contact = args.contact;
    for (i, step) in steps.iter().enumerate() {
        if i > 0 {
            std::thread::sleep(args.delay);
        }
        let payload = match step.as_str() {
            "info" => contact.info_xml(),
            "replace" => {
                contact.freq_10hz += 100;
                contact.replace_xml()
            }
            "delete" => contact.delete_xml(),
            "radioinfo" => radioinfo_xml(),
            _ => MALFORMED.to_string(),
        };
        if let Err(e) = send(args.port, payload.as_bytes()) {
            eprintln!("fake-logger: couldn't send {step}: {e}");
            return ExitCode::FAILURE;
        }
        println!(
            "sent {step} to 127.0.0.1:{} ({} {} {} {} id {})",
            args.port,
            contact.mycall,
            contact.call,
            contact.mode,
            contact.timestamp.format("%Y-%m-%d %H:%M:%S"),
            contact.id.as_deref().unwrap_or("none")
        );
    }
    ExitCode::SUCCESS
}
