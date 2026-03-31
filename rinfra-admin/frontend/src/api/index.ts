import axios from 'axios'

const api = axios.create({
  baseURL: '/api/admin',
  timeout: 10000,
})

api.interceptors.request.use((config) => {
  const token = localStorage.getItem('admin_token')
  if (token) {
    config.headers.Authorization = `Bearer ${token}`
  }
  return config
})

api.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response?.status === 401) {
      localStorage.removeItem('admin_token')
      window.location.hash = '#/login'
    }
    return Promise.reject(error)
  },
)

export interface ApiResponse<T> {
  code: string
  data: T
  message: string
}

export interface SystemInfo {
  name: string
  version: string
  rust_version: string
  os: string
  arch: string
}

export interface HealthStatus {
  status: string
  uptime_secs: number
}

export interface ConfigSummary {
  app_name: string
  app_version: string
  http_host: string
  http_port: number
  cluster_mode: string
  cluster_role: string
  plugins_enabled: string[]
}

export interface PluginInfo {
  name: string
  category: string
  status: string
}

export interface ClusterNodeInfo {
  id: string
  role: string
  address: string
  status: string
  services: string[]
}

export interface ClusterNodeView {
  mode: string
  nodes: ClusterNodeInfo[]
}

export interface MenuEntry {
  path: string
  name: string
  icon: string
  category: string
  data_url: string
}

export interface MetricValue {
  labels: Record<string, string>
  value: number
}

export interface MetricEntry {
  name: string
  type: string
  help: string
  values: MetricValue[]
}

export const getInfo = () => api.get<ApiResponse<SystemInfo>>('/info')
export const getHealth = () => api.get<ApiResponse<HealthStatus>>('/health')
export const getConfig = () => api.get<ApiResponse<ConfigSummary>>('/config')
export const getPlugins = () => api.get<ApiResponse<PluginInfo[]>>('/plugins')
export const getClusterNodes = () => api.get<ApiResponse<ClusterNodeView>>('/cluster/nodes')
export const getMetrics = () => api.get<ApiResponse<MetricEntry[]>>('/metrics')
export const getExtensions = () => api.get<ApiResponse<MenuEntry[]>>('/extensions')
export const fetchGenericData = (url: string) => api.get<ApiResponse<any>>(url.replace('/api/admin', ''))
