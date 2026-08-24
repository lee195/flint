<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { deleteModel, engineStatus, launchOllama } from "@/lib/tauri-commands";
import type { EngineStatusView, ModelInfo } from "@/lib/types";
import { usePolling } from "@/composables/use-polling";

const { t } = useI18n();
const status = ref<EngineStatusView | null>(null);
const error = ref("");
const confirmingDelete = ref<string | null>(null);
const launching = ref(false);

async function refresh() {
  try {
    status.value = await engineStatus();
    error.value = "";
  } catch (e) {
    error.value = String(e);
  }
}

usePolling(refresh, 5000);

async function doLaunch() {
  launching.value = true;
  try {
    await launchOllama();
    await new Promise((r) => setTimeout(r, 1500));
    await refresh();
  } catch (e) {
    error.value = String(e);
  } finally {
    launching.value = false;
  }
}

async function doDelete(model: ModelInfo) {
  try {
    await deleteModel(model.name);
    confirmingDelete.value = null;
    await refresh();
  } catch (e) {
    error.value = String(e);
  }
}

function formatSize(bytes: number): string {
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}
</script>

<template>
  <section class="mx-auto max-w-xl px-6 py-10">
    <h1 class="text-xl font-semibold">{{ t("settings.title") }}</h1>
    <p class="mt-2 text-sm text-muted-foreground">{{ t("settings.body") }}</p>

    <p v-if="error" class="mt-4 text-sm text-red-600">{{ error }}</p>

    <!-- Engine status pill -->
    <div class="mt-6 rounded-lg border border-border p-4">
      <h2 class="text-sm font-semibold">{{ t("settings.title") }} — engine</h2>
      <div class="mt-3 flex items-center gap-3">
        <span
          v-if="status?.health.state === 'Running'"
          class="inline-flex items-center gap-2 rounded-full bg-emerald-100 px-3 py-1 text-xs font-medium text-emerald-800"
        >
          <span class="h-2 w-2 rounded-full bg-emerald-500"></span>
          {{ t("settings.engine.running") }}
          <span class="text-emerald-600">
            · {{ t("settings.engine.version") }} {{ status.health.version }}
          </span>
        </span>
        <span
          v-else
          class="inline-flex items-center gap-2 rounded-full bg-amber-100 px-3 py-1 text-xs font-medium text-amber-800"
        >
          <span class="h-2 w-2 rounded-full bg-amber-500"></span>
          {{ t("settings.engine.notRunning") }}
        </span>
        <button
          v-if="status && status.health.state !== 'Running'"
          class="rounded-md bg-foreground px-3 py-1.5 text-xs font-medium text-background disabled:opacity-50"
          :disabled="launching"
          @click="doLaunch"
        >
          {{ launching ? t("settings.engine.launching") : t("settings.engine.launch") }}
        </button>
      </div>
      <p v-if="status && status.health.state !== 'Running'" class="mt-2 text-xs text-muted-foreground">
        {{ t("settings.engine.launchHint") }}
      </p>
    </div>

    <!-- Models on disk -->
    <div class="mt-6 rounded-lg border border-border p-4">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold">{{ t("settings.models.title") }}</h2>
        <span
          v-if="status?.recommendedInstalled"
          class="rounded-full bg-accent px-2 py-0.5 text-xs font-medium"
        >
          {{ t("settings.models.recommended") }}
        </span>
      </div>
      <ul v-if="status?.models.length" class="mt-3 divide-y divide-border">
        <li v-for="m in status.models" :key="m.name" class="flex items-center justify-between py-2">
          <div class="min-w-0">
            <p class="truncate text-sm font-medium">{{ m.name }}</p>
            <p class="text-xs text-muted-foreground">
              {{ formatSize(m.sizeBytes) }} {{ t("settings.models.onDisk") }}
            </p>
          </div>
          <button
            class="ml-4 shrink-0 text-xs text-muted-foreground underline"
            @click="confirmingDelete = m.name"
          >
            {{ t("settings.models.delete") }}
          </button>
        </li>
      </ul>
      <p v-else class="mt-3 text-sm text-muted-foreground">{{ t("settings.models.none") }}</p>

      <!-- Inline confirm (WKWebView rule: no modal dialogs) -->
      <div
        v-if="confirmingDelete"
        class="mt-3 rounded-md border border-amber-300 bg-amber-50 p-3"
      >
        <p class="text-sm">{{ t("settings.models.confirm", { model: confirmingDelete }) }}</p>
        <div class="mt-2 flex gap-2">
          <button
            class="rounded-md bg-red-600 px-3 py-1.5 text-xs font-medium text-white"
            @click="doDelete(status!.models.find((m) => m.name === confirmingDelete)!)"
          >
            {{ t("settings.models.confirmYes") }}
          </button>
          <button
            class="rounded-md bg-accent px-3 py-1.5 text-xs font-medium"
            @click="confirmingDelete = null"
          >
            {{ t("settings.models.confirmNo") }}
          </button>
        </div>
      </div>
    </div>
  </section>
</template>
