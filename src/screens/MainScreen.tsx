import Box from "@mui/material/Box";
import Stack from "@mui/material/Stack";
import Tab from "@mui/material/Tab";
import Tabs from "@mui/material/Tabs";
import { useState, type ReactNode } from "react";
import type { Run } from "../App";
import { ContactsCard } from "../components/ContactsCard";
import { FeedCard } from "../components/FeedCard";
import { SettingsCard } from "../components/SettingsCard";
import { UpdateBanner } from "../components/UpdateBanner";
import { copy } from "../copy";
import type { ShellView } from "../lib/types";

type TabId = "contacts" | "dx";

/** A tab's panel. Only the selected one renders its content. */
function Panel({ id, selected, children }: { id: TabId; selected: TabId; children: ReactNode }) {
  return (
    <Box
      component="main"
      id={`panel-${id}`}
      role="tabpanel"
      aria-labelledby={`tab-${id}`}
      hidden={id !== selected}
      sx={{ flexGrow: 1, px: 3, pt: 2, pb: 3 }}
    >
      {id === selected && children}
    </Box>
  );
}

/**
 * Signed in: the Contacts tab (the logging feed, Contacts seen and the settings) and the DX
 * cluster tab.
 */
export function MainScreen({ view, run }: { view: ShellView; run: Run }) {
  const [tab, setTab] = useState<TabId>("contacts");
  return (
    <>
      <Box sx={{ px: 1, borderBottom: 1, borderColor: "divider" }}>
        <Tabs value={tab} onChange={(_, next: TabId) => setTab(next)} aria-label={copy.tabs.label}>
          <Tab value="contacts" label={copy.tabs.contacts} id="tab-contacts" aria-controls="panel-contacts" />
          <Tab value="dx" label={copy.tabs.dxCluster} id="tab-dx" aria-controls="panel-dx" />
        </Tabs>
      </Box>
      <Panel id="contacts" selected={tab}>
        <Stack spacing={2}>
          <UpdateBanner update={view.update} run={run} />
          <FeedCard feed={view.feed} account={view.account} run={run} />
          <ContactsCard rows={view.stream} testMode={view.feed.test_mode} />
          <SettingsCard settings={view.settings} version={view.version} run={run} />
        </Stack>
      </Panel>
      {/* The DX cluster tab's content comes with the local DX cluster. */}
      <Panel id="dx" selected={tab}>
        {null}
      </Panel>
    </>
  );
}
