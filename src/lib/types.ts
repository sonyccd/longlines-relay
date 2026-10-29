// The JSON the Rust shell sends: `ShellView` in src-tauri/src/view.rs, which flattens the core's
// `Snapshot` (crates/relay-core/src/state.rs). `src/test/fixtures/snapshot.json` is written by
// the core's `ui_shape` test, so a change on the Rust side breaks the page's tests too.

export interface Account {
  callsign: string;
  email: string;
}

export type ListenerStatus =
  | { state: "listening"; port: number }
  | { state: "port_in_use"; port: number }
  | { state: "stopped"; port: number };

export type FeedStatus = "receiving" | "waiting" | "ignoring_contacts" | "port_in_use" | "test_mode";

export interface FeedView {
  status: FeedStatus;
  /** The chip's words, from the core. */
  label: string;
  listener: ListenerStatus;
  port: number;
  /** RFC 3339 UTC. */
  last_packet_at: string | null;
  dropped_packets: number;
  /** The station callsign of the latest contact that failed the callsign check. */
  mismatch: string | null;
  test_mode: boolean;
}

export type UpstreamStatus =
  | { state: "signed_out" }
  | { state: "connected" }
  | { state: "retrying"; at: string; reason: string }
  | { state: "rate_limited"; until: string };

export type PairingFailure = "expired" | "denied" | "unreachable" | "refused";

export type PairingView =
  | { state: "idle" }
  | { state: "starting" }
  | {
      state: "waiting";
      user_code: string;
      verification_uri: string;
      verification_uri_complete: string;
      /** RFC 3339 UTC. */
      expires_at: string;
    }
  | { state: "failed"; failure: PairingFailure };

export interface RowStatus {
  kind: string;
}

export interface StreamRow {
  id: number;
  kind: "info" | "replace" | "delete" | null;
  call: string;
  band: string | null;
  mode: string;
  frequency_hz: number | null;
  logged_at: string | null;
  received_at: string;
  /** "W4ABC · 40 m CW"; empty for a packet that couldn't be read. */
  title: string;
  /** "14:22 UTC". */
  time_utc: string;
  status: RowStatus;
  /** The status line, from the core. */
  status_text: string;
}

export type ClusterStatus = "off" | "listening" | "running" | "port_in_use" | "scope_missing";

/** The spots stream from Long Lines (contract section 9). */
export type ClusterUpstream =
  | { state: "off" }
  | { state: "connecting" }
  | { state: "connected" }
  | { state: "retrying"; at: string };

export interface ClusterAddresses {
  /** Always "127.0.0.1". */
  local: string;
  /** This computer's network address, only while other computers may connect. */
  lan: string | null;
}

export interface ClusterView {
  /** The DX cluster switch. */
  enabled: boolean;
  status: ClusterStatus;
  /** The chip's words, from the core. */
  label: string;
  listener: ListenerStatus;
  /** The bound port, else the configured one. */
  port: number;
  /** "Let other computers on this network connect". */
  lan: boolean;
  addresses: ClusterAddresses;
  /** RFC 3339 UTC. */
  last_sent_at: string | null;
  /** Spots sent since midnight UTC, test spots included. */
  sent_today: number;
  /** Programs connected to the cluster. */
  clients: number;
  upstream: ClusterUpstream;
}

export interface SentRow {
  id: number;
  /** The spot's id in Long Lines; null for a test spot. */
  spot_id: number | null;
  callsign: string;
  frequency_khz: number;
  mode: string | null;
  /** Program first, "POTA K-0039". */
  reference: string | null;
  connection: string;
  source_label: string;
  /** RFC 3339 UTC. */
  sent_at: string;
  /** "K4ABC · 14062.0 kHz CW". */
  title: string;
  /** "POTA K-0039 · from POTA states, CW", or "Test spot sent from this relay". */
  detail: string;
  /** "21:14 UTC". */
  time_utc: string;
  /** The relay's own test spot. */
  test: boolean;
}

export interface Snapshot {
  account: Account | null;
  feed: FeedView;
  upstream: UpstreamStatus;
  pairing: PairingView;
  /** Newest first. */
  stream: StreamRow[];
  /** The DX cluster tab. */
  cluster: ClusterView;
  /** Spots sent to logging software, newest first. */
  sent: SentRow[];
}

export type UpdateView =
  | { state: "none" }
  | { state: "available"; version: string }
  /** A .deb install can't update itself; the page links to the release (`url`, opened by the shell). */
  | { state: "manual"; version: string; url: string }
  | { state: "installing"; version: string }
  | { state: "failed"; version: string };

export interface SettingsView {
  port: number;
  autostart: boolean;
}

export interface ShellView extends Snapshot {
  /** A token is stored, so the main screen shows (the account may not be known yet). */
  signed_in: boolean;
  settings: SettingsView;
  update: UpdateView;
  version: string;
}
