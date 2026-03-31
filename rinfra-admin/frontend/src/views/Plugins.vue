<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { getPlugins, type PluginInfo } from '../api'

const plugins = ref<PluginInfo[]>([])
const loading = ref(true)
const error = ref('')
const filterCategory = ref('')

const categories = computed(() => {
  const cats = new Set(plugins.value.map(p => p.category))
  return ['', ...Array.from(cats).sort()]
})

const filtered = computed(() => {
  if (!filterCategory.value) return plugins.value
  return plugins.value.filter(p => p.category === filterCategory.value)
})

onMounted(async () => {
  try {
    const res = await getPlugins()
    plugins.value = res.data.data
  } catch (e: any) {
    error.value = e.message || 'Failed to fetch plugins'
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <div class="plugins-page">
    <div class="header">
      <h2>Plugins</h2>
      <select v-model="filterCategory" class="filter">
        <option value="">All Categories</option>
        <option v-for="cat in categories.slice(1)" :key="cat" :value="cat">
          {{ cat }}
        </option>
      </select>
    </div>

    <div v-if="loading" class="loading">Loading...</div>
    <div v-else-if="error" class="error-msg">{{ error }}</div>

    <div v-else class="plugin-grid">
      <div v-for="plugin in filtered" :key="plugin.name" class="plugin-card">
        <div class="plugin-name">{{ plugin.name }}</div>
        <div class="plugin-meta">
          <span class="badge category">{{ plugin.category }}</span>
          <span class="badge status" :class="plugin.status">{{ plugin.status }}</span>
        </div>
      </div>
    </div>

    <div v-if="!loading && !error" class="summary">
      {{ filtered.length }} plugin(s) shown
    </div>
  </div>
</template>

<style scoped>
.plugins-page h2 {
  margin: 0;
  font-size: 1.5rem;
  color: #1a1a2e;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 24px;
}

.filter {
  padding: 6px 12px;
  border: 1px solid #ddd;
  border-radius: 6px;
  font-size: 0.85rem;
  background: #fff;
}

.plugin-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 12px;
}

.plugin-card {
  background: #fff;
  border-radius: 10px;
  padding: 16px 20px;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.06);
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.plugin-name {
  font-weight: 600;
  color: #1a1a2e;
  font-size: 0.95rem;
}

.plugin-meta {
  display: flex;
  gap: 6px;
}

.badge {
  font-size: 0.7rem;
  padding: 2px 8px;
  border-radius: 10px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.badge.category {
  background: #e8eaff;
  color: #5c63c7;
}

.badge.status.enabled {
  background: #e8f5e9;
  color: #2e7d32;
}

.badge.status.available {
  background: #fff3e0;
  color: #e65100;
}

.summary {
  margin-top: 16px;
  font-size: 0.8rem;
  color: #888;
}

.loading { color: #888; font-size: 0.9rem; }
.error-msg { color: #f44336; background: #fff0f0; padding: 12px 16px; border-radius: 8px; }
</style>
