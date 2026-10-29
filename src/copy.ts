// Every word the window shows. Strings come from Brad's brief and the page 2 mockups; the ones
// marked "Invented" are not in either and are Brad's to reword. Row statuses and the feed chip's
// label come from the core (crates/relay-core/src/status.rs and state.rs).

export const copy = {
  appName: "Long Lines Relay",
  setUpLogging: "Set up logging software",

  signedOut: {
    heading: "Sign in to start relaying",
    body: "The relay sends the contacts you log to Long Lines as spots. Sign in with your Long Lines account to connect this computer.",
    steps: (port: number) => [
      {
        title: "Sign in with Long Lines",
        text: "Your browser opens so you can approve this computer. You don't type a password here.",
      },
      { title: "Set up your logging software", text: `Point its contact broadcast at this computer, port ${port}.` },
      { title: "Log contacts as usual", text: "Each contact shows up here and on Long Lines a moment later." },
    ],
    signIn: "Sign in with Long Lines",
    noAccount: "No account?",
    createAccount: "Create one on Long Lines",
    failures: {
      expired: {
        title: "The code expired",
        text: "It wasn't approved within 10 minutes. Sign in again to get a new code.",
      },
      // Invented: the mockups draw only the expired code.
      denied: {
        title: "Sign-in was denied",
        text: "Deny was selected in the browser, so this computer isn't connected. Sign in again to get a new code.",
      },
      // Invented.
      unreachable: {
        title: "Couldn't reach Long Lines",
        text: "Check this computer's internet connection, then sign in again.",
      },
      // Invented.
      refused: {
        title: "Long Lines didn't accept the sign-in",
        text: "Sign in again. If it keeps happening, check for a newer version of the relay.",
      },
    },
  },

  signingIn: {
    heading: "Finish signing in in your browser",
    body: "Long Lines opened in your browser. Check that it shows this code, then select Allow.",
    yourCode: "Your code",
    expiresIn: (left: string) => `Expires in ${left}`,
    waiting: "Waiting for you to approve",
    cancel: "Cancel",
    openAgain: "Open browser again",
    browserDidntOpen: "Browser didn't open? Go to",
    enterTheCode: "and enter the code.",
  },

  account: {
    heading: "Long Lines account",
    connected: "Connected",
    signedInAs: "Signed in as",
    signOut: "Sign out",
    privacy: "Your logged contacts are posted to Long Lines as spots.",
  },

  feed: {
    heading: "Logging feed",
    lastContact: (ago: string) => `Last contact received ${ago}`,
    nothingYet: "Nothing received yet",
    listening: (port: number) => `Listening on UDP port ${port}`,
    portInUse: (port: number) => `Port ${port} is in use by another program`,
    // Invented: the brief asks for a way to change the port but gives no words.
    portInUseHelp: "Choose another port here and in your logging software.",
    waitingHelp: (port: number) =>
      `Turn on contact broadcasts in your logging software and send them to this computer on port ${port}.`,
    mismatchTitle: (mycall: string) => `Contacts are logged as ${mycall}`,
    mismatchText: (account: string, mycall: string) =>
      `This account is ${account}, so contacts logged as ${mycall} are ignored. Change the callsign in your logging software, or sign in with the account for ${mycall}.`,
    testModeOn: "Test mode is on. Contacts aren't being sent to Long Lines.",
    testMode: "Test mode",
    testModeHelp:
      "Turn on test mode to make sure everything is working. Contacts show here and aren't sent to Long Lines.",
  },

  contacts: {
    heading: "Contacts seen",
    subtitle: "Everything the relay receives, newest first.",
    subtitleTestMode: "Everything the relay receives, newest first. In test mode it shows what would have happened.",
    empty: "Contacts you log will show up here.",
  },

  // Invented: the brief lists the settings (UDP port, "Start when I log in") but no screen.
  settings: {
    heading: "Settings",
    port: "UDP port",
    changePort: "Change port",
    badPort: "Enter a port from 1 to 65535.",
    autostart: "Start when I log in",
    version: (v: string) => `Version ${v}`,
  },

  // Invented: the brief asks for self-update but draws no screen for it.
  update: {
    available: (v: string) => `Version ${v} of the relay is available.`,
    install: "Install and restart",
    installing: (v: string) => `Installing version ${v}. The relay restarts when it's done.`,
    failed: (v: string) => `Couldn't install version ${v}.`,
    retry: "Try again",
    // Invented: a copy the updater can't replace (an .rpm or a bare binary) gets a link to the release.
    manual: (v: string) => `Version ${v} of the relay is available. Download the new package to install it.`,
    download: "Download",
  },
} as const;
