import CheckIcon from "@mui/icons-material/Check";
import ContentCopyIcon from "@mui/icons-material/ContentCopy";
import IconButton from "@mui/material/IconButton";
import Tooltip from "@mui/material/Tooltip";
import { useEffect, useState } from "react";
import { copy } from "../copy";

/** How long "Copied" shows after a copy. */
const COPIED_MS = 1500;

/** Copies `text` to the clipboard and says so for a moment. `label` names what it copies. */
export function CopyButton({ text, label, size }: { text: string; label: string; size: "small" | "medium" }) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), COPIED_MS);
    return () => window.clearTimeout(timer);
  }, [copied]);
  const onClick = () => {
    navigator.clipboard.writeText(text).then(
      () => setCopied(true),
      (error: unknown) => {
        // The webview may refuse the clipboard; the value is still on screen to copy by hand.
        console.error(error);
      },
    );
  };
  const fontSize = size === "small" ? "inherit" : "small";
  return (
    <Tooltip
      title={copy.cluster.copied}
      open={copied}
      describeChild
      disableFocusListener
      disableHoverListener
      disableTouchListener
    >
      <IconButton size="small" aria-label={label} onClick={onClick}>
        {copied ? <CheckIcon fontSize={fontSize} /> : <ContentCopyIcon fontSize={fontSize} />}
      </IconButton>
    </Tooltip>
  );
}
