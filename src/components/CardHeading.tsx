import Stack from "@mui/material/Stack";
import Typography from "@mui/material/Typography";
import type { ReactNode } from "react";

/** A card's h2, with an optional chip on the right. */
export function CardHeading({ id, title, chip }: { id: string; title: string; chip?: ReactNode }) {
  return (
    <Stack direction="row" spacing={1} sx={{ alignItems: "center", justifyContent: "space-between" }}>
      <Typography variant="h6" component="h2" id={id}>
        {title}
      </Typography>
      {chip}
    </Stack>
  );
}
