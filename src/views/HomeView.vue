<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import {
  cancelInstall,
  getInstallProgress,
  getRecommendation,
  startInstall,
} from "@/lib/tauri-commands";
import type { Recommendation } from "@/lib/types";

type Step =
  | { kind: "probing" }
  | { kind: "unsupported" }
  | { kind: "honesty" }
  | { kind: "recommend" }
  | { kind: "installing"; percent: number; status: string }
  | { kind: "cancelled" }
  | { kind: "ready" }
  | { kind: "failed"; error: string };

const { t } = useI18n();
const step = ref<Step>({ kind: "probing" });
const rec = ref<Recommendation | null>(null);
let installTimer: ReturnType<typeof setInterval> | null = null;

async function load() {
  step.value = { kind: "probing" };
  try {
    rec.value = await getRecommendation();
    if (!rec.value.probe.supported || !rec.value.model) {
      step.value = { kind: "unsupported" };
    } else {
      step.value = { kind: "honesty" };
    }
  } catch (e) {
    step.value = { kind: "failed", error: String(e) };
  }
}

onMounted(load);

function stopPolling() {
  if (installTimer) {
    clearInterval(installTimer);
    installTimer = null;
  }
}

onUnmounted(stopPolling);

function showRecommendation() {
  step.value = { kind: "recommend" };
}

function toReady() {
  stopPolling();
  step.value = { kind: "ready" };
}

async function doInstall() {
  const model = rec.value?.model?.tag;
  if (!model) return;
  try {
    await startInstall(model);
  } catch (e) {
    step.value = { kind: "failed", error: String(e) };
    return;
  }
  step.value = { kind: "installing", percent: 0, status: "starting" };
  installTimer = setInterval(async () => {
    try {
      const state = await getInstallProgress();
      if (state.state === "Running") {
        step.value = {
          kind: "installing",
          percent: state.progress.percent,
          status: state.progress.status,
        };
      } else if (state.state === "Done") {
        toReady();
      } else if (state.state === "Failed") {
        stopPolling();
        step.value = { kind: "failed", error: state.error };
      } else if (state.state === "Cancelled") {
        stopPolling();
        step.value = { kind: "cancelled" };
      }
    } catch {
      // transient poll errors are ignored — the next tick retries
    }
  }, 500);
}

async function doCancel() {
  try {
    await cancelInstall();
  } catch {
    // ignore
  }
  stopPolling();
  step.value = { kind: "cancelled" };
}

function formatSize(gb: number): string {
  return `${gb.toFixed(1)} GB`;
}
</script>

<template>
  <section class="mx-auto max-w-xl px-6 py-10">
    <h1 class="text-xl font-semibold">{{ t("home.title") }}</h1>
    <p class="mt-2 text-sm text-muted-foreground">{{ t("home.body") }}</p>

    <div v-if="step.kind === 'probing'" class="mt-6 text-sm text-muted-foreground">
      {{ t("home.probing") }}
    </div>

    <div v-else-if="step.kind === 'unsupported'" class="mt-6 rounded-lg border border-border bg-accent p-4">
      <h2 class="text-base font-semibold">{{ t("home.unsupported.title") }}</h2>
      <p class="mt-1 text-sm text-muted-foreground">{{ t("home.unsupported.body") }}</p>
    </div>

    <div v-else-if="step.kind === 'honesty'" class="mt-6 rounded-lg border border-border bg-accent p-4">
      <h2 class="text-base font-semibold">{{ t("home.honesty.title") }}</h2>
      <p class="mt-1 text-xs text-muted-foreground">
        {{ t("home.honesty.chip") }} {{ rec?.probe.chip }} · {{ rec?.probe.ramGb }} GB
      </p>
      <p class="mt-3 text-sm">{{ t("home.honesty.strengths") }}</p>
      <p class="mt-2 text-sm text-muted-foreground">{{ t("home.honesty.limits") }}</p>
      <button
        class="mt-4 rounded-md bg-foreground px-4 py-2 text-sm font-medium text-background"
        @click="showRecommendation"
      >
        {{ t("home.honesty.continue") }}
      </button>
    </div>

    <div v-else-if="step.kind === 'recommend' && rec?.model" class="mt-6">
      <h2 class="text-base font-semibold">{{ t("home.recommend.title") }}</h2>
      <p class="mt-1 text-sm text-muted-foreground">{{ t("home.recommend.lede") }}</p>
      <div class="mt-4 rounded-lg border border-border p-4">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-lg font-semibold">{{ rec.model.tag }}</p>
            <p class="text-xs text-muted-foreground">{{ rec.model.family }}</p>
          </div>
          <span class="rounded-full bg-accent px-3 py-1 text-xs font-medium">
            {{ rec.probe.tier }}
          </span>
        </div>
        <dl class="mt-4 grid grid-cols-2 gap-x-4 gap-y-2 text-sm">
          <div>
            <dt class="text-xs text-muted-foreground">{{ t("home.recommend.size") }}</dt>
            <dd>{{ formatSize(rec.model.sizeGb) }}</dd>
          </div>
          <div>
            <dt class="text-xs text-muted-foreground">{{ t("home.recommend.license") }}</dt>
            <dd>{{ rec.model.license }}</dd>
          </div>
          <div class="col-span-2">
            <dt class="text-xs text-muted-foreground">{{ t("home.recommend.source") }}</dt>
            <dd class="truncate">{{ rec.model.source }}</dd>
          </div>
        </dl>
        <p class="mt-3 text-xs text-muted-foreground">
          {{ t("home.recommend.asOf") }} {{ rec.asOf }} · {{ t("home.recommend.agentLocked") }}
        </p>
        <div class="mt-4">
          <button
            v-if="rec.alreadyInstalled"
            class="rounded-md bg-foreground px-4 py-2 text-sm font-medium text-background"
            @click="toReady"
          >
            {{ t("home.recommend.installed") }} → {{ t("home.recommend.continue") }}
          </button>
          <button
            v-else
            class="rounded-md bg-foreground px-4 py-2 text-sm font-medium text-background"
            @click="doInstall"
          >
            {{ t("home.recommend.download") }}
          </button>
        </div>
      </div>
    </div>

    <div v-else-if="step.kind === 'installing'" class="mt-6">
      <h2 class="text-base font-semibold">{{ t("home.installing.title") }}</h2>
      <div class="mt-3 h-2 w-full overflow-hidden rounded-full bg-accent">
        <div
          class="h-full bg-foreground transition-all"
          :style="{ width: Math.min(step.percent, 100) + '%' }"
        ></div>
      </div>
      <p class="mt-2 text-xs text-muted-foreground">
        {{ t("home.installing.status") }} — {{ Math.round(step.percent) }}%
      </p>
      <button class="mt-3 text-sm text-muted-foreground underline" @click="doCancel">
        {{ t("home.installing.cancel") }}
      </button>
    </div>

    <div v-else-if="step.kind === 'cancelled'" class="mt-6 rounded-lg border border-border bg-accent p-4">
      <h2 class="text-base font-semibold">{{ t("home.cancelled.title") }}</h2>
      <p class="mt-1 text-sm text-muted-foreground">{{ t("home.cancelled.body") }}</p>
      <button
        class="mt-4 rounded-md bg-foreground px-4 py-2 text-sm font-medium text-background"
        @click="load"
      >
        {{ t("home.cancelled.ok") }}
      </button>
    </div>

    <div v-else-if="step.kind === 'ready' && rec?.model" class="mt-6 rounded-lg border border-border bg-accent p-4">
      <h2 class="text-base font-semibold">{{ t("home.ready.title") }}</h2>
      <p class="mt-1 text-sm text-muted-foreground">
        {{ t("home.ready.body", { model: rec.model.tag }) }}
      </p>
      <RouterLink
        to="/chat"
        class="mt-4 inline-block rounded-md bg-foreground px-4 py-2 text-sm font-medium text-background"
      >
        {{ t("home.ready.openChat") }}
      </RouterLink>
    </div>

    <div v-else-if="step.kind === 'failed'" class="mt-6 rounded-lg border border-border bg-accent p-4">
      <h2 class="text-base font-semibold">{{ t("home.failed.title") }}</h2>
      <p class="mt-1 text-sm text-muted-foreground">{{ step.error }}</p>
      <button
        class="mt-4 rounded-md bg-foreground px-4 py-2 text-sm font-medium text-background"
        @click="load"
      >
        {{ t("home.failed.retry") }}
      </button>
    </div>
  </section>
</template>
