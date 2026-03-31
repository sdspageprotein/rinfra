<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { getMetrics, type MetricEntry } from '../api'

const metrics = ref<MetricEntry[]>([])
const loading = ref(true)
const error = ref('')
const lastUpdated = ref('')
let timer: ReturnType<typeof setInterval> | null = null

const filterText = ref('')

const categories = computed(() => {
  const map: Record<string, MetricEntry[]> = {}
  for (const m of metrics.value) {
    const prefix = m.name.split('_').slice(0, 2).join('_')
    const cat = categoryLabel(prefix)
    if (!map[cat]) map[cat] = []
    map[cat].push(m)
  }
  return map
})

const filteredCategories = computed(() => {
  const q = filterText.value.toLowerCase()
  if (!q) return categories.value
  const result: Record<string, MetricEntry[]> = {}
  for (const [cat, entries] of Object.entries(categories.value)) {
    const filtered = entries.filter(
      (e) =>
        e.name.toLowerCase().includes(q) ||
        cat.toLowerCase().includes(q) ||
        e.help.toLowerCase().includes(q),
    )
    if (filtered.length > 0) result[cat] = filtered
  }
  return result
})

function categoryLabel(prefix: string): string {
  const map: Record<string, string> = {
    http_requests: 'HTTP',
    http_request: 'HTTP',
    tcp_connections: 'TCP',
    tcp_bytes: 'TCP',
    cache_hits: 'Cache',
    cache_misses: 'Cache',
    mq_messages: 'Message Queue',
    circuit_breaker: 'Circuit Breaker',
    timer_executions: 'Timer',
    timer_execution: 'Timer',
    lock_acquisitions: 'Lock',
    lock_contention: 'Lock',
    db_pool: 'Database',
  }
  return map[prefix] || 'Other'
}

function formatValue(v: number): string {
  if (Number.isInteger(v)) return v.toLocaleString()
  if (v < 0.001) return v.toExponential(2)
  return v.toFixed(3)
}

function labelString(labels: Record<string, string>): string {
  const entries = Object.entries(labels)
  if (entries.length === 0) return ''
  return entries.map(([k, v]) => `${k}="${v}"`).join(', ')
}

function typeIcon(t: string): string {
  switch (t) {
    case 'counter': return '↑'
    case 'gauge': return '⊘'
    case 'histogram': return '▤'
    default: return '•'
  }
}

async function fetchData() {
  try {
    const res = await getMetrics()
    metrics.value = res.data.data || []
    lastUpdated.value = new Date().toLocaleTimeString()
    error.value = ''
  } catch (e: any) {
    error.value = e.message || 'Failed to fetch metrics'
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  fetchData()
  timer = setInterval(fetchData, 5000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <div class="metrics-page">
    <div class="header">
      <h2>Metrics</h2>
      <div class="header-right">
        <input
          v-model="filterText"
          type="text"
          placeholder="Filter metrics..."
          class="filter-input"
        />
        <span v-if="lastUpdated" class="last-updated">{{ lastUpdated }}</span>
      </div>
    </div>

    <div v-if="loading" class="loading">Loading...</div>
    <div v-else-if="error" class="error-msg">{{ error }}</div>

    <div v-else-if="metrics.length === 0" class="empty">
      <p>No metrics available. Make sure <code>plugins.metrics.enabled: true</code> in your config.</p>
    </div>

    <div v-else class="categories">
      <div
        v-for="(entries, cat) in filteredCategories"
        :key="cat"
        class="category-section"
      >
        <h3 class="category-title">{{ cat }}</h3>
        <div class="metric-grid">
          <div v-for="metric in entries" :key="metric.name" class="metric-card">
            <div class="metric-header">
              <span class="metric-name">{{ metric.name }}</span>
              <span class="metric-type" :title="metric.type">{{ typeIcon(metric.type) }} {{ metric.type }}</span>
            </div>
            <div v-if="metric.help" class="metric-help">{{ metric.help }}</div>
            <div class="metric-values">
              <div
                v-for="(mv, idx) in metric.values"
                :key="idx"
                class="metric-value-row"
              >
                <span class="metric-labels">{{ labelString(mv.labels) || '(no labels)' }}</span>
                <span class="metric-val">{{ formatValue(mv.value) }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.metrics-page h2 {
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

.header-right {
  display: flex;
  align-items: center;
  gap: 12px;
}

.filter-input {
  padding: 6px 12px;
  border: 1px solid #ddd;
  border-radius: 6px;
  font-size: 0.85rem;
  background: #fff;
  width: 200px;
}

.last-updated {
  font-size: 0.75rem;
  color: #999;
  white-space: nowrap;
}

.categories {
  display: flex;
  flex-direction: column;
  gap: 28px;
}

.category-title {
  font-size: 1rem;
  font-weight: 600;
  color: #5c63c7;
  margin: 0 0 12px;
  padding-bottom: 6px;
  border-bottom: 2px solid #e8eaff;
}

.metric-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(360px, 1fr));
  gap: 12px;
}

.metric-card {
  background: #fff;
  border-radius: 10px;
  padding: 16px 20px;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.06);
}

.metric-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 4px;
}

.metric-name {
  font-weight: 600;
  font-size: 0.9rem;
  color: #1a1a2e;
  font-family: 'SF Mono', 'Consolas', 'Monaco', monospace;
}

.metric-type {
  font-size: 0.7rem;
  padding: 2px 8px;
  border-radius: 10px;
  background: #f0f0f5;
  color: #666;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.metric-help {
  font-size: 0.78rem;
  color: #999;
  margin-bottom: 8px;
}

.metric-values {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.metric-value-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 4px 8px;
  background: #f9f9fc;
  border-radius: 6px;
  font-size: 0.82rem;
}

.metric-labels {
  color: #888;
  font-family: 'SF Mono', 'Consolas', 'Monaco', monospace;
  font-size: 0.75rem;
  max-width: 70%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.metric-val {
  font-weight: 700;
  color: #1a1a2e;
  font-family: 'SF Mono', 'Consolas', 'Monaco', monospace;
  font-size: 0.9rem;
}

.empty {
  background: #fff;
  border-radius: 10px;
  padding: 32px;
  text-align: center;
  color: #888;
}

.empty code {
  background: #f0f0f5;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 0.85rem;
}

.loading { color: #888; font-size: 0.9rem; }
.error-msg { color: #f44336; background: #fff0f0; padding: 12px 16px; border-radius: 8px; }
</style>
