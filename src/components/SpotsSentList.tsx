import Box from "@mui/material/Box";
import Chip from "@mui/material/Chip";
import Divider from "@mui/material/Divider";
import List from "@mui/material/List";
import ListItem from "@mui/material/ListItem";
import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import { Fragment } from "react";
import { copy } from "../copy";
import type { SentRow } from "../lib/types";
import { CardHeading } from "./CardHeading";

/** Spots sent: the core's last 50 lines sent to logging software, newest first, test spots tagged. */
export function SpotsSentList({ rows }: { rows: SentRow[] }) {
  return (
    <Box component="section" aria-labelledby="sent-heading">
      <CardHeading id="sent-heading" title={copy.cluster.sent.heading} />
      <Typography variant="body2" color="text.secondary" sx={{ pb: 0.5 }}>
        {copy.cluster.sent.subtitle}
      </Typography>
      <List disablePadding aria-labelledby="sent-heading">
        {rows.map((row) => (
          <Fragment key={row.id}>
            <Divider component="li" aria-hidden />
            <ListItem disableGutters sx={{ py: 1.25, alignItems: "flex-start", gap: 2 }}>
              <Stack sx={{ minWidth: 0, flexGrow: 1 }}>
                <Typography variant="body1">{row.title}</Typography>
                <Typography variant="body2" color="text.secondary">
                  {row.detail}
                </Typography>
              </Stack>
              <Stack spacing={0.5} sx={{ alignItems: "flex-end", flex: "none" }}>
                <Typography variant="caption" color="text.secondary" noWrap>
                  {row.time_utc}
                </Typography>
                {row.test && <Chip label={copy.cluster.sent.test} size="small" variant="outlined" />}
              </Stack>
            </ListItem>
          </Fragment>
        ))}
      </List>
    </Box>
  );
}
