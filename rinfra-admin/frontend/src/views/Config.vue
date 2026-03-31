<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { getConfig, type ConfigSummary } from '../api'

const config = ref<ConfigSummary | null>(null)
const loading = ref(true)
const error = ref('')

onMounted(async () => {
  try {
    const res = await getConfig()
    config.value = res.data.data
  } catch (e: any) {
    error.value = e.message || 'Failed to fetch config'
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <div class="config-page">
    <h2>Configuration</h2>

    <div v-if="loading" class="loading">Loading...</div>
    <div v-else-if="error" class="error-msg">{{ error }}</div>

    <div v-else class="config-sections">
      <div class="section">
        <h3>Application</h3>
        <table class="config-table">
          <tr>
            <td class="key">Name</td>
            <td class="val">{{ config?.app_name }}</td>
          </tr>
          <tr>
            <td class="key">Version</td>
            <td class="val">{{ config?.app_version }}</td>
          </tr>
        </table>
      </div>

      <div class="section">
        <h3>HTTP Server</h3>
        <table class="config-table">
          <tr>
            <td class="key">Host</td>
            <td class="val"><code>{{ config?.http_host }}</code></td>
          </tr>
          <tr>
            <td class="key">Port</td>
            <td class="val"><code>{{ config?.http_port }}</code></td>
          </tr>
        </table>
      </div>

      <div class="section">
        <h3>Cluster</h3>
        <table class="config-table">
          <tr>
            <td class="key">Mode</td>
            <td class="val">
              <span class="cluster-mode-badge" :class="config?.cluster_mode ?? 'standalone'">
                {{ config?.cluster_mode ?? 'standalone' }}
              </span>
            </td>
          </tr>
          <tr>
            <td class="key">Role</td>
            <td class="val">{{ config?.cluster_role || '-' }}</td>
          </tr>
        </table>
      </div>

      <div class="section">
        <h3>Enabled Plugins</h3>
        <div v-if="config?.plugins_enabled.length" class="tag-list">
          <span v-for="p in config.plugins_enabled" :key="p" class="tag">{{ p }}</span>
        </div>
        <div v-else class="empty">No plugins enabled</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.config-page h2 {
  margin: 0 0 24px;
  font-size: 1.5rem;
  color: #1a1a2e;
}

.config-sections {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.section {
  background: #fff;
  border-radius: 12px;
  padding: 20px 24px;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.06);
}

.section h3 {
  margin: 0 0 12px;
  font-size: 1rem;
  color: #555;
  font-weight: 600;
}

.config-table {
  width: 100%;
  border-collapse: collapse;
}

.config-table td {
  padding: 8px 0;
  border-bottom: 1px solid #f0f0f0;
}

.config-table .key {
  color: #888;
  width: 140px;
  font-size: 0.85rem;
}

.config-table .val {
  color: #1a1a2e;
  font-weight: 500;
}

.config-table code {
  background: #f0f2ff;
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 0.85rem;
}

.tag-list {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.tag {
  background: #e8eaff;
  color: #5c63c7;
  padding: 4px 12px;
  border-radius: 14px;
  font-size: 0.8rem;
  font-weight: 500;
}

.cluster-mode-badge {
  display: inline-block;
  font-size: 0.7rem;
  padding: 2px 10px;
  border-radius: 10px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  font-weight: 600;
}

.cluster-mode-badge.standalone {
  background: #e8eaff;
  color: #5c63c7;
}

.cluster-mode-badge.cluster {
  background: #e8f5e9;
  color: #2e7d32;
}

.empty {
  color: #aaa;
  font-size: 0.85rem;
}

.loading { color: #888; font-size: 0.9rem; }
.error-msg { color: #f44336; background: #fff0f0; padding: 12px 16px; border-radius: 8px; }
</style>
