// The only module that talks to the Rust shell (src-tauri/src/commands.rs).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ShellView } from "./types";

/** The event the shell sends after every change. */
export const STATE_EVENT = "state";

export const relay = {
  getState: () => invoke<ShellView>("get_state"),
  startSignIn: () => invoke<ShellView>("start_sign_in"),
  openSignInPage: () => invoke<void>("open_sign_in_page"),
  cancelSignIn: () => invoke<ShellView>("cancel_sign_in"),
  signOut: () => invoke<ShellView>("sign_out"),
  setTestMode: (on: boolean) => invoke<ShellView>("set_test_mode", { on }),
  setPort: (port: number) => invoke<ShellView>("set_port", { port }),
  setAutostart: (on: boolean) => invoke<ShellView>("set_autostart", { on }),
  setClusterEnabled: (on: boolean) => invoke<ShellView>("set_cluster_enabled", { on }),
  setClusterPort: (port: number) => invoke<ShellView>("set_cluster_port", { port }),
  setClusterLan: (on: boolean) => invoke<ShellView>("set_cluster_lan", { on }),
  sendTestSpot: () => invoke<ShellView>("send_test_spot"),
  openSetupDocs: () => invoke<void>("open_setup_docs"),
  openSignUp: () => invoke<void>("open_sign_up"),
  openLogFolder: () => invoke<void>("open_log_folder"),
  installUpdate: () => invoke<void>("install_update"),
  openUpdatePage: () => invoke<void>("open_update_page"),
  onState: (handler: (view: ShellView) => void): Promise<UnlistenFn> =>
    listen<ShellView>(STATE_EVENT, (event) => handler(event.payload)),
};

export type Relay = typeof relay;
