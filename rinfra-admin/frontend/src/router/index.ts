import { createRouter, createWebHashHistory } from 'vue-router'
import Dashboard from '../views/Dashboard.vue'
import Login from '../views/Login.vue'
import Plugins from '../views/Plugins.vue'
import Config from '../views/Config.vue'
import Cluster from '../views/Cluster.vue'
import Metrics from '../views/Metrics.vue'
import GenericView from '../views/GenericView.vue'
import { getExtensions } from '../api'

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/login', name: 'Login', component: Login, meta: { public: true } },
    { path: '/', name: 'Dashboard', component: Dashboard },
    { path: '/plugins', name: 'Plugins', component: Plugins },
    { path: '/config', name: 'Config', component: Config },
    { path: '/cluster', name: 'Cluster', component: Cluster },
    { path: '/metrics', name: 'Metrics', component: Metrics },
  ],
})

router.beforeEach((to) => {
  if (to.meta.public) return true
  if (!localStorage.getItem('admin_token')) {
    return { name: 'Login' }
  }
  return true
})

let extensionsLoaded = false

export async function loadExtensions() {
  if (extensionsLoaded) return
  try {
    const resp = await getExtensions()
    const entries = resp.data.data || []
    for (const entry of entries) {
      router.addRoute({
        path: entry.path,
        name: entry.name,
        component: GenericView,
        meta: {
          title: entry.name,
          dataUrl: entry.data_url,
          icon: entry.icon,
          category: entry.category,
        },
      })
    }
    extensionsLoaded = true
  } catch (e) {
    console.warn('Failed to load admin extensions:', e)
  }
}

export default router
