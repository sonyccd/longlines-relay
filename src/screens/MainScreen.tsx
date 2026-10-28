import Stack from "@mui/material/Stack";
import type { Run } from "../App";
import { AccountCard } from "../components/AccountCard";
import { ContactsCard } from "../components/ContactsCard";
import { FeedCard } from "../components/FeedCard";
import { SettingsCard } from "../components/SettingsCard";
import { UpdateBanner } from "../components/UpdateBanner";
import type { ShellView } from "../lib/types";

/** Screen 3: the account, the logging feed, Contacts seen and the settings. */
export function MainScreen({ view, run }: { view: ShellView; run: Run }) {
  return (
    <Stack component="main" spacing={2} sx={{ p: 3 }}>
      <UpdateBanner update={view.update} run={run} />
      <AccountCard account={view.account} run={run} />
      <FeedCard feed={view.feed} account={view.account} run={run} />
      <ContactsCard rows={view.stream} testMode={view.feed.test_mode} />
      <SettingsCard settings={view.settings} version={view.version} run={run} />
    </Stack>
  );
}
