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
      settings: "Settings",
    },
    home: {
      title: "Welcome to Flint",
      body: "Flint will probe this Mac, recommend one model that fits it, and let you chat with it — all locally, no cloud.",
      bridge: "Bridge test:",
    },
    settings: {
      title: "Settings",
      body: "Engine status and model management land here in Phase 0.",
    },
    status: {
      ready: "Ready",
    },
  },
};

export default createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: "en",
  messages,
});
