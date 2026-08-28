import { createI18n } from "vue-i18n";

// en-first, i18n-ready (flint doc 00): every chrome string lives in this catalog so a
// `de` locale is a catalog pass, not a refactor. Default locale en; no locale switching
// in v0. The catalog is the ONLY place UI strings are defined.
const messages = {
  en: {
    app: {
      name: "Flint",
      tagline: "Local AI for the rest of us",
    },
    nav: {
      home: "Start",
      chat: "Chat",
      agent: "Agent",
      settings: "Settings",
    },
    home: {
      title: "Welcome to Flint",
      body: "Flint runs local AI on this Mac — no cloud, no accounts. Your data stays on the device.",
      probing: "Looking at this Mac…",
      unsupported: {
        title: "Flint needs Apple Silicon",
        body: "This Mac uses an Intel chip, which Flint doesn't support yet — local models wouldn't be satisfying on it. A chat website will do more for you today.",
      },
      honesty: {
        title: "What local AI can do",
        chip: "Your Mac:",
        strengths:
          "Local models run entirely on this Mac. Your files and messages never leave the device — great for private drafting, summarizing, and working offline.",
        limits:
          "They're not ChatGPT. Expect shallower reasoning on hard problems and slower answers on big documents. In exchange: private, offline, and yours.",
        continue: "Show me what fits this Mac",
      },
      recommend: {
        title: "One model for this Mac",
        lede: "No catalog, no jargon. Based on this hardware, we recommend exactly one model.",
        size: "Size on disk",
        license: "License",
        source: "Source",
        asOf: "Recommendations as of",
        agentLocked: "Agent mode (files, commands) is on the way in a later phase.",
        download: "Download",
        installed: "Installed — ready to chat",
        continue: "Continue",
      },
      installing: {
        title: "Downloading",
        status: "Downloading",
        cancel: "Cancel",
      },
      ready: {
        title: "You're ready",
        body: "{model} is installed and runs on this Mac — offline and private.",
        openChat: "Open chat",
      },
      failed: {
        title: "Something went wrong",
        retry: "Try again",
      },
      cancelled: {
        title: "Download cancelled",
        body: "Nothing was changed — the download can be resumed later.",
        ok: "OK",
      },
    },
    settings: {
      title: "Settings",
      body: "Engine status and model management.",
      engine: {
        running: "Engine running",
        version: "version",
        notRunning: "Engine not running",
        launch: "Start engine",
        launchHint: "Starts the bundled engine (llama.cpp) with the recommended model.",
        launching: "Starting…",
      },
      models: {
        title: "Models on disk",
        none: "No models on disk.",
        recommended: "recommended for this Mac",
        onDisk: "on disk",
        delete: "Delete",
        confirm: "Delete {model}?",
        confirmYes: "Delete",
        confirmNo: "Keep",
      },
      error: "Something went wrong loading settings.",
    },
    chat: {
      title: "Chat",
      runsOnDevice: "runs on this device",
      placeholder: "Ask your model…",
      send: "Send",
      stop: "Stop",
      newChat: "New chat",
      confirmNew: "Start a new chat? The current conversation will be cleared.",
      confirmYes: "Start new",
      confirmNo: "Keep",
      engineDown: "The engine isn't running — replies need it.",
      launch: "Start engine",
      error: "Something went wrong.",
      empty: "Ask anything — it runs on this Mac.",
    },
    agent: {
      title: "Agent mode",
      lockedTitle: "Agent mode is locked",
      smokeRun: "Run capability test",
      smokeRunning: "Testing…",
      smokePassed: "Capability test passed",
      smokeFailed: "Capability test failed",
      smokeDetail: "Detail:",
      lockedReason: "Why:",
      workspace: "Workspace",
      chooseFolder: "Choose folder",
      pasteHint: "or paste a path",
      pastePlaceholder: "/path/to/workspace",
      add: "Use",
      promptPlaceholder: "Tell the agent what to do in this folder…",
      run: "Run",
      stop: "Stop",
      noWorkspace: "Choose a folder the agent may work in first.",
      empty: "Agent mode lets a local model read, write, and run commands in a folder you choose — every action asks your permission first.",
      toolCall: "tool",
      toolResult: "done",
      toolFailed: "failed",
      allow: "Allow",
      deny: "Deny",
      permissionTitle: "Allow this action?",
      cancelled: "Run stopped.",
      review: "Review",
      restore: "Restore snapshot",
      restored: "Workspace restored to before this run.",
      error: "Something went wrong.",
      running: "running",
    },
    common: {
      retry: "Try again",
    },
  },
};

export default createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: "en",
  messages,
});
