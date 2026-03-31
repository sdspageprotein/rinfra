<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { RouterLink, RouterView, useRoute, useRouter } from 'vue-router'
import { getExtensions, type MenuEntry } from './api'
import { loadExtensions } from './router'

const extensions = ref<MenuEntry[]>([])
const route = useRoute()
const router = useRouter()

const isLoginPage = computed(() => route.name === 'Login')

function handleLogout() {
  localStorage.removeItem('admin_token')
  router.push({ name: 'Login' })
}

onMounted(async () => {
  if (isLoginPage.value) return
  await loadExtensions()
  try {
    const resp = await getExtensions()
    extensions.value = resp.data.data || []
  } catch {
    // extensions not available — show built-in menu only
  }
})
</script>

<template>
  <RouterView v-if="isLoginPage" />
  <div v-else class="app">
    <aside class="sidebar">
      <div class="logo">
        <h1>rinfra</h1>
        <span class="subtitle">Admin</span>
      </div>
      <nav>
        <RouterLink to="/" class="nav-item">
          <span class="icon">&#9673;</span> Dashboard
        </RouterLink>
        <RouterLink to="/plugins" class="nav-item">
          <span class="icon">&#9881;</span> Plugins
        </RouterLink>
        <RouterLink to="/config" class="nav-item">
          <span class="icon">&#9776;</span> Config
        </RouterLink>
        <RouterLink to="/cluster" class="nav-item">
          <span class="icon">&#9670;</span> Cluster
        </RouterLink>
        <RouterLink to="/metrics" class="nav-item">
          <span class="icon">&#9636;</span> Metrics
        </RouterLink>

        <template v-if="extensions.length > 0">
          <div class="nav-divider"></div>
          <RouterLink
            v-for="ext in extensions"
            :key="ext.path"
            :to="ext.path"
            class="nav-item"
          >
            <span class="icon" v-html="ext.icon"></span> {{ ext.name }}
          </RouterLink>
        </template>
      </nav>
      <div class="sidebar-footer">
        <button class="logout-btn" @click="handleLogout">Logout</button>
      </div>
    </aside>
    <main class="content">
      <RouterView />
    </main>
  </div>
</template>

<style scoped>
.app {
  display: flex;
  min-height: 100vh;
}

.sidebar {
  width: 220px;
  background: #1a1a2e;
  color: #e0e0e0;
  padding: 0;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
}

.logo {
  padding: 24px 20px 16px;
  border-bottom: 1px solid #2a2a4a;
}

.logo h1 {
  margin: 0;
  font-size: 1.5rem;
  font-weight: 700;
  color: #7c83ff;
}

.logo .subtitle {
  font-size: 0.75rem;
  color: #888;
  text-transform: uppercase;
  letter-spacing: 2px;
}

nav {
  padding: 12px 0;
  display: flex;
  flex-direction: column;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px 20px;
  color: #b0b0c0;
  text-decoration: none;
  font-size: 0.9rem;
  transition: background 0.15s, color 0.15s;
}

.nav-item:hover {
  background: #2a2a4a;
  color: #fff;
}

.nav-item.router-link-exact-active {
  background: #2a2a4a;
  color: #7c83ff;
  border-left: 3px solid #7c83ff;
}

.nav-divider {
  height: 1px;
  background: #2a2a4a;
  margin: 8px 16px;
}

.icon {
  font-size: 1.1rem;
}

.sidebar-footer {
  margin-top: auto;
  padding: 16px 20px;
  border-top: 1px solid #2a2a4a;
}

.logout-btn {
  width: 100%;
  padding: 8px;
  background: transparent;
  border: 1px solid #444;
  border-radius: 6px;
  color: #b0b0c0;
  font-size: 0.85rem;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}

.logout-btn:hover {
  background: #e53935;
  border-color: #e53935;
  color: #fff;
}

.content {
  flex: 1;
  padding: 32px;
  background: #f5f6fa;
  overflow-y: auto;
}
</style>
