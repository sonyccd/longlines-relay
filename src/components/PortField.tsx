import Button from "@mui/material/Button";
import Stack from "@mui/material/Stack";
import TextField from "@mui/material/TextField";
import { useState, type FormEvent } from "react";
import { copy } from "../copy";
import { parsePort } from "../lib/port";

/**
 * A port and Change port: the UDP port for contacts, or the DX cluster's port. `onChange` gets
 * a valid port that differs from `port`. The relay never picks another port by itself.
 */
export function PortField({
  id,
  label,
  port,
  onChange,
}: {
  id: string;
  label: string;
  port: number;
  onChange: (port: number) => void;
}) {
  const [text, setText] = useState(String(port));
  const parsed = parsePort(text);
  const invalid = text.trim() !== "" && parsed === null;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (parsed !== null && parsed !== port) onChange(parsed);
  };
  return (
    <Stack component="form" direction="row" spacing={1} sx={{ alignItems: "flex-start" }} onSubmit={submit} noValidate>
      <TextField
        id={id}
        label={label}
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
