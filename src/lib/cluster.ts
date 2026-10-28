// Page-side rules for the DX cluster tab. Pure, so tests pass the state.

import type { ClusterView, SentRow } from "./types";

/**
 * Whether to say that no destination sends spots here yet. The relay can't see destinations, so
 * the page decides: the cluster is listening, the spots stream is connected, and nothing but the
 * relay's own test spots has been sent.
 */
export function noDestinationYet(cluster: ClusterView, sent: SentRow[]): boolean {
  return (
    (cluster.status === "listening" || cluster.status === "running") &&
    cluster.upstream.state === "connected" &&
    !sent.some((row) => !row.test)
  );
}
