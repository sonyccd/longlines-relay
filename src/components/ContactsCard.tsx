import Card from "@mui/material/Card";
import CardContent from "@mui/material/CardContent";
import Divider from "@mui/material/Divider";
import List from "@mui/material/List";
import ListItem from "@mui/material/ListItem";
import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import { Fragment } from "react";
import { copy } from "../copy";
import type { StreamRow } from "../lib/types";
import { CardHeading } from "./CardHeading";

/** Contacts seen: the core's last 50 rows, newest first. */
export function ContactsCard({ rows, testMode }: { rows: StreamRow[]; testMode: boolean }) {
  return (
    <Card variant="outlined" component="section" aria-labelledby="contacts-heading">
      <CardContent>
        <Stack spacing={0.5}>
          <CardHeading id="contacts-heading" title={copy.contacts.heading} />
          {rows.length === 0 ? (
            <Typography variant="body2" color="text.secondary" align="center" sx={{ py: 4 }}>
              {copy.contacts.empty}
            </Typography>
          ) : (
            <>
              <Typography variant="body2" color="text.secondary" sx={{ pb: 1 }}>
                {testMode ? copy.contacts.subtitleTestMode : copy.contacts.subtitle}
              </Typography>
              <List disablePadding aria-labelledby="contacts-heading">
                {rows.map((row) => (
                  <Fragment key={row.id}>
                    <Divider component="li" aria-hidden />
                    <ListItem disableGutters sx={{ py: 1.5, alignItems: "flex-start", gap: 2 }}>
                      <Stack sx={{ minWidth: 0, flexGrow: 1 }}>
                        {row.title ? (
                          <>
                            <Typography variant="body1">{row.title}</Typography>
                            <Typography variant="body2" color="text.secondary">
                              {row.status_text}
                            </Typography>
                          </>
                        ) : (
                          <Typography variant="body1">{row.status_text}</Typography>
                        )}
                      </Stack>
                      <Typography variant="body2" color="text.secondary" noWrap>
                        {row.time_utc}
                      </Typography>
                    </ListItem>
                  </Fragment>
                ))}
              </List>
            </>
          )}
        </Stack>
      </CardContent>
    </Card>
  );
}
