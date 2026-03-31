<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { getClusterNodes, type ClusterNodeView } from '../api'

const data = ref<ClusterNodeView | null>(null)
const loading = ref(true)
const error = ref('')
let timer: ReturnType<typeof setInterval> | null = null

const isCluster = computed(() => data.value?.mode === 'cluster')

async function fetchNodes() {
  try {
    const res = await getClusterNodes()
    data.value = res.data.data
    error.value = ''
  } catch (e: any) {
    error.value = e.message || 'Failed to fetch cluster nodes'
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  fetchNodes()
  timer = setInterval(fetchNodes, 5000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <div class="cluster-page">
    <div class="header">
      <h2>Cluster</h2>
      <span v-if="data" class="mode-badge" :class="data.mode">
        {{ data.mode }}
      </span>
    </div>

    <div v-if="loading" class="loading">Loading...</div>
    <div v-else-if="error" class="error-msg">{{ error }}</div>

    <template v-else>
      <div v-if="!isCluster" class="standalone-info">
        <div class="info-icon">&#9432;</div>
        <div>
          <div class="info-title">Standalone Mode</div>
          <div class="info-desc">This node is running in standalone mode. No cluster nodes to display.</div>
        </div>
      </div>

      <div v-else>
        <div class="stats">
          <div class="stat-card">
            <div class="stat-label">Total Nodes</div>
            <div class="stat-value">{{ data?.nodes.length ?? 0 }}</div>
          </div>
          <div class="stat-card online">
            <div class="stat-label">Online</div>
            <div class="stat-value">{{ data?.nodes.filter(n => n.status === 'Online').length ?? 0 }}</div>
          </div>
          <div class="stat-card offline">
            <div class="stat-label">Offline</div>
            <div class="stat-value">{{ data?.nodes.filter(n => n.status === 'Offline').length ?? 0 }}</div>
          </div>
        </div>

        <div class="nodes-table-wrap">
          <table v-if="data?.nodes.length" class="nodes-table">
            <thead>
              <tr>
                <th>Node ID</th>
                <th>Role</th>
                <th>Address</th>
                <th>Status</th>
                <th>Services</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="node in data.nodes" :key="node.id">
                <td class="node-id"><code>{{ node.id }}</code></td>
                <td><span class="role-badge" :class="node.role.toLowerCase()">{{ node.role }}</span></td>
                <td><code>{{ node.address }}</code></td>
                <td><span class="status-dot" :class="node.status.toLowerCase()"></span> {{ node.status }}</td>
                <td>
                  <span v-for="svc in node.services" :key="svc" class="svc-tag">{{ svc }}</span>
                  <span v-if="!node.services.length" class="empty-svc">-</span>
                </td>
              </tr>
            </tbody>
          </table>
          <div v-else class="no-nodes">No nodes registered yet.</div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.cluster-page h2 {
  margin: 0;
  font-size: 1.5rem;
  color: #1a1a2e;
}

.header {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 24px;
}

.mode-badge {
  font-size: 0.7rem;
  padding: 3px 10px;
  border-radius: 10px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  font-weight: 600;
}

.mode-badge.standalone {
  background: #e8eaff;
  color: #5c63c7;
}

.mode-badge.cluster {
  background: #e8f5e9;
  color: #2e7d32;
}

.standalone-info {
  display: flex;
  align-items: flex-start;
  gap: 14px;
  background: #fff;
  border-radius: 12px;
  padding: 24px;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.06);
  border-left: 4px solid #7c83ff;
}

.info-icon {
  font-size: 1.5rem;
  color: #7c83ff;
  line-height: 1;
}

.info-title {
  font-weight: 600;
  color: #1a1a2e;
  margin-bottom: 4px;
}

.info-desc {
  font-size: 0.85rem;
  color: #888;
}

.stats {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 16px;
  margin-bottom: 24px;
}

.stat-card {
  background: #fff;
  border-radius: 12px;
  padding: 20px;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.06);
  text-align: center;
}

.stat-card.online { border-top: 3px solid #4caf50; }
.stat-card.offline { border-top: 3px solid #f44336; }

.stat-label {
  font-size: 0.75rem;
  color: #888;
  text-transform: uppercase;
  letter-spacing: 1px;
  margin-bottom: 8px;
}

.stat-value {
  font-size: 2rem;
  font-weight: 700;
  color: #1a1a2e;
}

.nodes-table-wrap {
  background: #fff;
  border-radius: 12px;
  padding: 4px;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.06);
  overflow-x: auto;
}

.nodes-table {
  width: 100%;
  border-collapse: collapse;
}

.nodes-table th {
  text-align: left;
  padding: 12px 16px;
  font-size: 0.75rem;
  color: #888;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  border-bottom: 1px solid #f0f0f0;
}

.nodes-table td {
  padding: 12px 16px;
  font-size: 0.9rem;
  color: #1a1a2e;
  border-bottom: 1px solid #f8f8f8;
}

.nodes-table code {
  background: #f0f2ff;
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 0.8rem;
}

.node-id code {
  font-weight: 500;
}

.role-badge {
  font-size: 0.7rem;
  padding: 2px 8px;
  border-radius: 8px;
  font-weight: 600;
  text-transform: uppercase;
}

.role-badge.main { background: #fff3e0; color: #e65100; }
.role-badge.worker { background: #e3f2fd; color: #1565c0; }

.status-dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  margin-right: 6px;
}

.status-dot.online { background: #4caf50; }
.status-dot.offline { background: #f44336; }
.status-dot.draining { background: #ff9800; }

.svc-tag {
  background: #f0f2ff;
  color: #5c63c7;
  padding: 2px 8px;
  border-radius: 8px;
  font-size: 0.75rem;
  margin-right: 4px;
}

.empty-svc {
  color: #ccc;
}

.no-nodes {
  padding: 40px;
  text-align: center;
  color: #aaa;
  font-size: 0.9rem;
}

.loading { color: #888; font-size: 0.9rem; }
.error-msg { color: #f44336; background: #fff0f0; padding: 12px 16px; border-radius: 8px; }
</style>
