import type { ChipProps } from "@mui/material/Chip";
import { copy } from "../copy";
import type { UpstreamStatus } from "./types";

/** The header chip for the connection to Long Lines; none while signed out. */
export function upstreamChip(upstream: UpstreamStatus): Pick<ChipProps, "label" | "color"> | null {
  switch (upstream.state) {
    case "connected":
      return { label: copy.header.connected, color: "success" };
    case "retrying":
      return { label: copy.header.retrying, color: "warning" };
    case "rate_limited":
      return { label: copy.header.rateLimited, color: "warning" };
    case "credential_store_unavailable":
      return { label: copy.header.credentialStore, color: "error" };
    case "signed_out":
      return null;
  }
}
