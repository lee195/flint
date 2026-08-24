<script setup lang="ts">
import { useI18n } from "vue-i18n";

const { t } = useI18n();
</script>

<template>
  <div class="flex h-screen flex-col overflow-hidden bg-background text-foreground">
    <header class="flex items-center justify-between border-b border-border px-4 py-3">
      <div class="flex items-center gap-4">
        <span class="text-sm font-semibold tracking-tight">{{ t("app.name") }}</span>
        <nav class="flex items-center gap-2 text-xs">
          <RouterLink
            to="/"
            class="text-muted-foreground hover:text-foreground"
            active-class="text-foreground font-medium"
          >
            {{ t("nav.home") }}
          </RouterLink>
          <RouterLink
            to="/settings"
            class="text-muted-foreground hover:text-foreground"
            active-class="text-foreground font-medium"
          >
            {{ t("nav.settings") }}
          </RouterLink>
        </nav>
      </div>
      <span class="text-xs text-muted-foreground">{{ t("app.tagline") }}</span>
    </header>

    <main
      id="main-content"
      tabindex="-1"
      class="min-w-0 w-full flex-1 overflow-y-auto focus:outline-none"
    >
      <router-view />
    </main>
  </div>
</template>
