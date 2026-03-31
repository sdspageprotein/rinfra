<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { getInfo, getHealth, getConfig, type SystemInfo, type HealthStatus, type ConfigSummary } from '../api'

const info = ref<SystemInfo | null>(null)
const health = ref<HealthStatus | null>(null)
const config = ref<ConfigSummary | null>(null)
const loading = ref(true)
const error = ref('')

const uptimeDisplay = computed(() => {
  const secs = health.value?.uptime_secs ?? 0
  if (secs < 60) return `${secs}s`
  if (secs < 3600) return `${Math.floor(secs / 60)}m ${secs % 60}s`
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  return `${h}h ${m}m`
})

onMounted(async () => {
  try {
    const [infoRes, healthRes, configRes] = await Promise.all([getInfo(), getHealth(), getConfig()])
    info.value = infoRes.data.data
    health.value = healthRes.data.data
    config.value = configRes.data.data
  } catch (e: any) {
    error.value = e.message || 'Failed to fetch data'
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <div class="dashboard">
    <h2>Dashboard</h2>

    <div v-if="loading" class="loading">Loading...</div>
    <div v-else-if="error" class="error-msg">{{ error }}</div>

    <div v-else class="cards">
      <div class="card health-card" :class="health?.status === 'healthy' ? 'healthy' : 'unhealthy'">
        <div class="card-label">Status</div>
        <div class="card-value">{{ health?.status ?? 'Unknown' }}</div>
      </div>

      <div class="card uptime-card">
        <div class="card-label">Uptime</div>
        <div class="card-value">{{ uptimeDisplay }}</div>
        <div class="card-sub">{{ health?.uptime_secs ?? 0 }} seconds</div>
      </div>

      <div class="card cluster-card" :class="config?.cluster_mode ?? 'standalone'">
        <div class="card-label">Cluster</div>
        <div class="card-value cluster-mode">{{ config?.cluster_mode ?? 'standalone' }}</div>
        <div v-if="config?.cluster_role" class="card-sub">Role: {{ config.cluster_role }}</div>
      </div>

      <div class="card">
        <div class="card-label">Application</div>
        <div class="card-value">{{ info?.name }}</div>
        <div class="card-sub">v{{ info?.version }}</div>
      </div>

      <div class="card">
        <div class="card-label">Platform</div>
        <div class="card-value">{{ info?.os }} / {{ info?.arch }}</div>
      </div>

      <div class="card">
        <div class="card-label">Rust Edition</div>
        <div class="card-value">{{ info?.rust_version }}</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.dashboard h2 {
  margin: 0 0 24px;
  font-size: 1.5rem;
  color: #1a1a2e;
}

.cards {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 16px;
}

.card {
  background: #fff;
  border-radius: 12px;
  padding: 20px;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.06);
}

.card-label {
  font-size: 0.75rem;
  color: #888;
  text-transform: uppercase;
  letter-spacing: 1px;
  margin-bottom: 8px;
}

.card-value {
  font-size: 1.25rem;
  font-weight: 600;
  color: #1a1a2e;
}

.card-sub {
  font-size: 0.85rem;
  color: #aaa;
  margin-top: 4px;
}

.health-card.healthy {
  border-left: 4px solid #4caf50;
}

.health-card.unhealthy {
  border-left: 4px solid #f44336;
}

.uptime-card {
  border-left: 4px solid #2196f3;
}

.cluster-card {
  border-left: 4px solid #9e9e9e;
}

.cluster-card.cluster {
  border-left-color: #4caf50;
}

.cluster-mode {
  text-transform: capitalize;
}

.loading {
  color: #888;
  font-size: 0.9rem;
}

.error-msg {
  color: #f44336;
  background: #fff0f0;
  padding: 12px 16px;
  border-radius: 8px;
}
</style>
