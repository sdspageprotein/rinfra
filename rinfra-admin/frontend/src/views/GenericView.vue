<script setup lang="ts">
import { ref, onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'
import { fetchGenericData } from '../api'

const route = useRoute()
const loading = ref(true)
const error = ref('')
const title = ref('')
const data = ref<any>(null)

async function loadData() {
  loading.value = true
  error.value = ''
  const dataUrl = route.meta.dataUrl as string
  title.value = (route.meta.title as string) || route.name as string || 'Extension'
  if (!dataUrl) {
    error.value = 'No data_url configured for this page'
    loading.value = false
    return
  }
  try {
    const resp = await fetchGenericData(dataUrl)
    data.value = resp.data.data
  } catch (e: any) {
    error.value = e.message || 'Failed to fetch data'
  } finally {
    loading.value = false
  }
}

onMounted(loadData)
watch(() => route.path, loadData)
</script>

<template>
  <div class="generic-view">
    <h2>{{ title }}</h2>

    <div v-if="loading" class="loading">Loading...</div>
    <div v-else-if="error" class="error-msg">{{ error }}</div>

    <template v-else>
      <!-- Array data → table -->
      <div v-if="Array.isArray(data) && data.length > 0" class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th v-for="key in Object.keys(data[0])" :key="key">{{ key }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(row, i) in data" :key="i">
              <td v-for="key in Object.keys(data[0])" :key="key">
                {{ typeof row[key] === 'object' ? JSON.stringify(row[key]) : row[key] }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- Object data → cards -->
      <div v-else-if="data && typeof data === 'object' && !Array.isArray(data)" class="cards">
        <div v-for="(val, key) in data" :key="String(key)" class="card">
          <div class="card-label">{{ key }}</div>
          <div class="card-value">
            {{ typeof val === 'object' ? JSON.stringify(val) : val }}
          </div>
        </div>
      </div>

      <!-- Empty array -->
      <div v-else-if="Array.isArray(data) && data.length === 0" class="empty">
        No data available
      </div>

      <!-- Scalar / string -->
      <div v-else class="scalar">
        <pre>{{ JSON.stringify(data, null, 2) }}</pre>
      </div>
    </template>
  </div>
</template>

<style scoped>
.generic-view h2 {
  margin: 0 0 24px;
  font-size: 1.5rem;
  color: #1a1a2e;
}

.table-wrap {
  overflow-x: auto;
}

.data-table {
  width: 100%;
  border-collapse: collapse;
  background: #fff;
  border-radius: 8px;
  overflow: hidden;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.06);
}

.data-table th {
  background: #f0f1f5;
  text-align: left;
  padding: 10px 14px;
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: #666;
  border-bottom: 2px solid #e0e0e0;
}

.data-table td {
  padding: 10px 14px;
  border-bottom: 1px solid #f0f0f0;
  font-size: 0.9rem;
  color: #333;
}

.data-table tbody tr:hover {
  background: #f8f9fc;
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
  font-size: 1.1rem;
  font-weight: 600;
  color: #1a1a2e;
  word-break: break-all;
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

.empty {
  color: #999;
  text-align: center;
  padding: 40px;
}

.scalar pre {
  background: #fff;
  padding: 16px;
  border-radius: 8px;
  overflow-x: auto;
  font-size: 0.85rem;
}
</style>
