# Set up logging software

Long Lines Relay listens for the contact broadcasts your logging software sends on this
computer, in the N1MM Logger+ format, and posts each contact to Long Lines as a spot. Your
logger needs three things set:

1. **Contact broadcasts on.** Each saved contact goes out as a small network packet.
2. **Sent to this computer on the relay's port.** The address is `127.0.0.1` and the port is
   `12060` unless you changed it in the relay's settings. The relay shows its port under
   "Logging feed".
3. **Your station callsign as the logged callsign.** The relay only posts contacts logged under
   the callsign of the Long Lines account it is signed in to. Portable forms such as
   `W4/KK4PWJ`, `KK4PWJ/P`, `KK4PWJ/M` and `KK4PWJ/QRP` count as the same station, and the call
   is posted exactly as you logged it.

Then log a contact. It shows up under "Contacts seen" in the relay, and on Long Lines a moment
later. Turn on test mode in the relay first if you want to check the setup without posting
anything.

| Logger | Works with the relay | Notes |
| --- | --- | --- |
| N1MM Logger+ | Yes | New contacts, edits and deletes. |
| DXLog.net | Yes, with "Use N1MM QSO format" on | Its own (traditional) format is not compatible. |
| MacLoggerDX | Version 6.58 or later | Set the broadcast address to `127.0.0.1`. Not yet tested with a real packet. |
| Not1MM | New contacts only, with two settings changed | Sends the operator, not the station callsign, and uses port 12061 by default. Edits and deletes are not sent. |

Other loggers that send N1MM-format `contactinfo` packets should work. The relay does not read
WSJT-X's own UDP format, ADIF files or log files, and it ignores packets from other computers.

The relay can also go the other way: its [DX cluster](#dx-cluster) shows the spots that match
your Long Lines connections in your logger's bandmap.

## N1MM Logger+

1. Open **Config > Configure Ports, Mode Control, Winkey, etc.** and select the **Broadcast
   Data** tab.
2. Tick **Contacts** and enter `127.0.0.1:12060` beside it. If something else already listens
   for contacts, add the relay after it with a space between the two addresses.
3. Select **OK**.

Edits and deletes in N1MM reach Long Lines too: an edited contact updates its spot, and a
deleted contact's spot is removed. In a multi-computer N1MM network, each computer's relay posts
only the contacts logged on that computer; contacts forwarded from other stations are ignored
("Ignored. Forwarded by another N1MM station.").

## DXLog.net

1. Open **Options > Broadcast**.
2. Turn on the contact (QSO) broadcast to `127.0.0.1`, port `12060`.
3. Tick **Use N1MM QSO format**.

Without "Use N1MM QSO format", DXLog.net sends its traditional format, which the relay can't
use: those contacts show as "Couldn't read packet" or never reach Long Lines.

## MacLoggerDX

MacLoggerDX sends N1MM-format contact packets from version 6.58 (November 2025). Update first
if you have an older version.

1. Turn on the N1MM-format contact broadcast in MacLoggerDX. Its help describes where.
2. MacLoggerDX sends to your network's broadcast address by default. To send to this computer
   only, quit MacLoggerDX, run this in Terminal, then start MacLoggerDX again:

   ```sh
   defaults write com.dogparksoftware.MacLoggerDX broadcast_ip_address 127.0.0.1
   ```

3. Make the port match the relay's. MacLoggerDX keeps it in its `n1mm_radio_port` setting; the
   MacLoggerDX help has the details.

## Not1MM

Not1MM sends N1MM-format packets, with two differences that matter to the relay:

- **The callsign it sends is the operator's**, not the station's. If the operator is someone
  else, the relay ignores the contacts ("Ignored. Logged as ..."). In **File > Station
  Settings**, leave **Operator** empty or set it to the station callsign.
- **It sends contacts to port 12061 by default**, not 12060. In **File > Configuration
  Settings**, on the **N1MM** tab, tick **Send N1MM packets**, tick the box beside **Contact**,
  and set the contact address to `127.0.0.1:12060`. Or change the relay's port to 12061 instead.

Not1MM sends a packet only when a contact is saved. Editing or deleting a contact in Not1MM
does not change or remove its spot on Long Lines.

Not1MM sends radio packets to port 12060 by default, so the relay can show "Receiving" while
contacts still go to 12061. If the relay shows "Receiving" but no contacts appear, check the
contact port.

## DX cluster

The relay's DX cluster shows Long Lines spots in your logging software's bandmap. Your logger
connects to the relay through its Telnet (DX cluster) window, as it would to a DXSpider node on
the internet, and the relay passes on each spot that one of your Long Lines connections matches.
It only receives: nothing your logger sends through the cluster reaches Long Lines, and spots
you enter in the logger go nowhere. Nothing tunes your radio.

### In Long Lines

1. Open **Destinations**, add a destination and pick **Local DX cluster**. Choose this
   computer's relay. The relay's DX cluster tab links there (**Add destination**).
2. Link the destination to the connections whose spots you want in the bandmap.

### In the relay

1. On the **DX cluster** tab, turn on **Local DX cluster**. Nothing listens on this computer
   until you do.
2. The tab shows the address and port your logger connects to: `127.0.0.1` and port `7373`.
   If another program already has port 7373, the relay listens on the next free port up to
   7399 and shows that one; use the port the tab shows. **Change port** picks a port of your
   own, which the relay then always uses.
3. To let a logger on another computer connect, tick **Let other computers on this network
   connect**. The tab then also shows this computer's network address (such as
   `192.168.1.42`) for the other computers to use. Anyone on your network can connect while
   it is on; there is no password.

**Set up logging software** on the tab shows the same steps, with copy buttons for the address,
port and callsign.

### N1MM Logger+

1. Open the Telnet window (**Window > Telnet**).
2. Add a new cluster with address `127.0.0.1` and port `7373` (or the port the relay shows).
   N1MM keeps its cluster list behind the **Edit** button next to the cluster drop-down.
3. Select the cluster and **Connect**. The relay asks `login:`; N1MM answers with your
   callsign, or type it. Any callsign is accepted. The relay then greets you:

   ```text
   Hello KK4PWJ, this is Long Lines Relay on SHACK-PC. Spots from your Long Lines connections appear here.
   KK4PWJ de LONGLINES 28-Oct-2026 2114Z >
   ```

4. Select **Send test spot** in the relay. A spot for `LLTEST` on 14050.0 kHz CW should show up
   in the Telnet window and in the bandmap:

   ```text
   DX de KK4PWJ:    14050.0  LLTEST       CW Long Lines Relay test spot  2102Z
   ```

**Filters.** N1MM Logger+ applies its own spot filters and band limits (the Telnet window's
filter settings and the bandmap's band). If a spot shows in the Telnet window but not in the
bandmap, or not at all, check those first. The test spot is on 20 m CW.

### Other software

Any logger that reads a DX cluster over Telnet should work: open its DX cluster or Telnet
settings, add a cluster with the relay's address and port, connect, and send a test spot. Each
spot arrives as one line in the layout DXSpider uses, with the mode as the first word of the
comment and then, depending on the destination's settings, the reference, the source and the
spotter's comment:

```text
DX de W4DOG:     14062.0  K4ABC        CW POTA K-0039                 2114Z
```

The relay answers `bye` and `quit` by closing the connection, ignores blank lines and the
commands loggers send on connect (such as `set/page 0` or `sh/dx`), and refuses spots entered
in the logger ("Spots can't be sent through Long Lines Relay.").

### When the DX cluster doesn't look right

- **"Listening" and "No destination sends spots to this computer yet".** The relay is
  connected to Long Lines, but nothing has arrived. Add a Local DX cluster destination for this
  relay in Long Lines and link it to a connection. Spots arrive only as they are spotted;
  there is no backlog.
- **"Port 7373 is in use by another program".** Another program has the port you chose with
  **Change port**, or, on the default port, every port from 7373 to 7399. Choose another port
  in the relay and in your logger.
- **"Sign in again to use the DX cluster".** This computer was signed in before the relay could
  receive spots. Select **Sign in again** and approve it in your browser.
- **"Couldn't reach Long Lines for spots".** The relay retries by itself. Your logger stays
  connected meanwhile and gets spots again once the relay is back.
- **Spots show in the relay's "Spots sent" but not in the logger.** Check that the logger is
  connected (the tab counts programs connected), then its spot filters and band limits.
- **A logger on another computer can't connect.** Tick **Let other computers on this network
  connect**, use the network address the tab shows, and check that this computer's firewall
  lets the relay accept connections on its port.

## When something doesn't look right

- **"Waiting for logging software" and "Nothing received yet".** Nothing has arrived on the
  relay's port. Check that broadcasts are on, the address is `127.0.0.1` and the port matches
  the one under "Logging feed".
- **"Port 12060 is in use by another program".** Another program already listens on that port.
  Choose another port in the relay, and use the same port in your logger.
- **"Ignoring contacts" and "Ignored. Logged as K4XYZ."** The logger's station callsign isn't
  the callsign of the account the relay is signed in to. Change it in the logger, or sign in
  with the account for that callsign.
- **"Ignored. No station callsign in the logger."** The logger sent the contact without a
  station callsign. Set your callsign in the logger's station settings.
- **"Not sent. Logged more than 30 minutes ago."** Only contacts from the last 30 minutes become
  spots. This happens when the relay was off or offline when you logged them, or when the
  contact's time is wrong.
- **"Not sent. Logged more than 5 minutes ahead of this computer's clock."** The contact's time
  is in the future. Check that the logger logs in UTC and that this computer's clock is right.
- **"Couldn't read packet".** The packet isn't in the N1MM format the relay reads.

## Verified logger facts

What the relay is built on, with where each fact comes from. "Capture wanted" means the fact is
from documentation or source code and no real packet from that logger has been recorded yet.

### N1MM Logger+

Source: [External UDP broadcasts](https://n1mmwp.hamdocs.com/appendices/external-udp-broadcasts/)
(N1MM Logger+ documentation).

- The packet's root element is `contactinfo` for a new contact, `contactreplace` for an edit
  and `contactdelete` for a delete. An edit sends `contactdelete` and then `contactreplace`, so
  a replace after a delete re-creates the spot.
- `rxfreq` and `txfreq` are in units of 10 Hz: `352519` is 3525.19 kHz.
- `timestamp` is `YYYY-MM-DD HH:MM:SS` with no time zone. N1MM logs in UTC. To confirm with a
  capture on Windows.
- `ID` identifies the contact. The documentation calls it "32 byte"; the sample is 32
  hexadecimal characters (a GUID without dashes).
- `IsOriginal` is `False` on a contact that a networked station is forwarding. The relay posts
  only contacts with `IsOriginal` true or absent.
- `StationName` is the computer that sent the packet, not necessarily the one the contact was
  logged on.
- `oldtimestamp` and `oldcall` exist for edits that change the time or the call.
- `band` is a decimal string (`3.5`). The relay works out the band from the frequency instead.

### DXLog.net

Source: [Additional Information](https://dxlog.net/docs/index.php/Additional_Information)
(DXLog.net documentation). Docs-verified, capture wanted.

- With **Options > Broadcast > Use N1MM QSO format**, the root element is `contactinfo`, `app`
  is `N1MM`, `ID` is 32 hexadecimal characters, and the core fields match N1MM's.
- The traditional format also uses the root `contactinfo` but has `qsoid`, `guid` and
  `stationid` instead of `ID`. It is not compatible with the relay.
- `band` uses the computer's decimal separator (a comma in many locales), another reason the
  relay works out the band from the frequency.

### MacLoggerDX

Source: the MacLoggerDX help ([dogparksoftware.com](https://www.dogparksoftware.com/)).
Capture wanted.

- `contactinfo` packets exist from version 6.58 (November 2025).
- Packets go to the Class C broadcast address by default.
  `defaults write com.dogparksoftware.MacLoggerDX broadcast_ip_address 127.0.0.1` sends them to
  this computer instead. The port is the `n1mm_radio_port` setting.
- The field list is not documented.
- Not yet confirmed: whether the relay, which listens on all of this computer's addresses and
  keeps only packets from this computer, receives MacLoggerDX's default broadcasts on macOS.

### Not1MM

Source: the Not1MM source code ([github.com/mbridak/not1mm](https://github.com/mbridak/not1mm)),
[`not1mm/lib/n1mm.py`](https://github.com/mbridak/not1mm/blob/master/not1mm/lib/n1mm.py) and
[`not1mm/__main__.py`](https://github.com/mbridak/not1mm/blob/master/not1mm/__main__.py).

- `app` is `NOT1MM`.
- It sends `contactinfo` only, when a contact is saved. Replace and delete are not wired up.
- `ID` is `uuid4().hex` (32 hexadecimal characters).
- `timestamp` is UTC.
- `rxfreq` and `txfreq` are the frequency in Hz with the last digit dropped, which is N1MM's
  10 Hz units.
- `mycall` is the operator (`current_op`), not the station callsign. The operator is the Station
  Settings **Operator** field, or the station callsign when that is empty.
- The default contact address is `127.0.0.1:12061`, not 12060. Radio packets default to
  `127.0.0.1:12060`.
- Compatible for new contacts when the operator is the station callsign and the contact port is
  the relay's.

### Still to confirm by hand

- The N1MM Logger+ timestamp's time zone, from a capture on Windows.
- That the relay receives MacLoggerDX's default (broadcast address) packets on macOS.
- A real packet from DXLog.net and from MacLoggerDX, kept as test fixtures.
- That N1MM Logger+ connects to the relay's DX cluster, accepts its `login:` prompt and
  greeting, and puts the test spot in the bandmap, from this computer and, with the network
  setting on, from a second computer.
- Whether N1MM Logger+ needs the mode word at the start of a spot's comment, and how it treats
  a spot whose comment has none.
- Where N1MM Logger+ keeps its cluster list (the **Edit** button above) and whether it answers
  `login:` with the callsign by itself.
