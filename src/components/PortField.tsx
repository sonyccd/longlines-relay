import Button from "@mui/material/Button";
import Stack from "@mui/material/Stack";
import TextField from "@mui/material/TextField";
import { useState, type FormEvent } from "react";
import type { Run } from "../App";
import { copy } from "../copy";
import { parsePort } from "../lib/port";
import { relay } from "../lib/relay";

/** The UDP port and Change port. The relay never picks another port by itself. */
export function PortField({ port, run, id }: { port: number; run: Run; id: string }) {
  const [text, setText] = useState(String(port));
  const parsed = parsePort(text);
  const invalid = text.trim() !== "" && parsed === null;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (parsed !== null && parsed !== port) run(() => relay.setPort(parsed));
  };
  return (
    <Stack component="form" direction="row" spacing={1} sx={{ alignItems: "flex-start" }} onSubmit={submit} noValidate>
      <TextField
        id={id}
        label={copy.settings.port}
        size="small"
        value={text}
        onChange={(e) => setText(e.target.value)}
        error={invalid}
        helperText={invalid ? copy.settings.badPort : undefined}
        slotProps={{ htmlInput: { inputMode: "numeric", maxLength: 5 } }}
        sx={{ width: 160 }}
      />
      <Button type="submit" variant="outlined" disabled={parsed === null || parsed === port}>
        {copy.settings.changePort}
      </Button>
    </Stack>
  );
}
