# Packet fixtures

Sample UDP payloads for the parser tests (`tests/fixtures.rs`). None of these is a capture yet:
every file is synthesized from documentation or source code, as the table says. Replace or add
captured packets as they come in (N1MM Logger+ on Windows is the first one wanted, to confirm the
timestamp zone and `IsOriginal`), and mark them "captured" here with the logger version.

| File | Origin | Notes |
| --- | --- | --- |
| `n1mm-contactinfo.xml` | Synthesized from the N1MM Logger+ appendix sample ("External UDP Broadcasts", n1mmwp.hamdocs.com/appendices/external-udp-broadcasts/) | The appendix's typos are corrected: the call `WlAW` (lower-case L) is `W1AW`, and the unclosed `<run1run2>1<run1run2>` is closed. `IsOriginal` is `True` here (the sample has `False`, which is the forwarded case below). `rxfreq` 352519 is 3525.19 kHz (10 Hz units). |
| `n1mm-contactinfo-forwarded.xml` | Same sample, `IsOriginal` `False` as printed in the appendix | A contact another networked N1MM station is forwarding. The relay drops it. |
| `n1mm-contactreplace.xml` | Synthesized from the appendix: same fields as `contactinfo`, root `contactreplace`, plus `oldtimestamp` / `oldcall` | The frequency moved up 1 kHz, as an edit would. Same `ID`. |
| `n1mm-contactdelete.xml` | Synthesized from the appendix's `contactdelete` field list | Only `app`, `timestamp`, `call`, `contestnr`, `StationName`, `ID`. |
| `n1mm-radioinfo.xml` | Synthesized from the appendix's `RadioInfo` sample | Not a contact; counts as activity and is otherwise ignored. |
| `dxlog-n1mm-contactinfo.xml` | Synthesized from DXLog.net's documentation of Options > Broadcast > "Use N1MM QSO format" | `app` is `N1MM`; `band` uses a comma decimal separator (`14,0`), which is why the band comes from the frequency. Capture wanted. |
| `not1mm-contactinfo.xml` | Synthesized from Not1MM's source (`not1mm/lib/n1mm.py`) | `app` is `NOT1MM`; `mycall` carries the operator (`current_op`), not the station call; `ID` is `uuid4().hex`; frequencies are Hz with the last digit dropped (10 Hz units). Not1MM sends only `contactinfo`. |
| `malformed-truncated.xml` | Synthesized | The sample cut off mid-element: must become "Couldn't read packet", never a crash. |
| `malformed-unclosed.xml` | Synthesized | The appendix's own unclosed `<run1run2>` typo, left in: not well-formed XML. |

MacLoggerDX has no fixture: its field list is undocumented. Capture wanted.

## DX cluster lines

| File | Origin | Notes |
| --- | --- | --- |
| `dxspider-lines.txt` | Captured: the 23 `DX de` lines of `supabase/functions/_shared/__tests__/fixtures/dxcluster-telnet.txt` in `sonyccd/longlines` (G1FEF-2, DXSpider 1.57, 2026-10-12), copied byte for byte | Each line ends with DXSpider's two BEL characters and LF, as captured. `tests/cluster.rs` parses each into its parts and checks the relay formats it back byte for byte (CR LF, no BEL), as contract section 10.3 requires. |
