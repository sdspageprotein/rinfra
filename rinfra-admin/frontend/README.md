# rinfra-admin frontend

rinfra 管理控制台前端，基于 Vue 3 + TypeScript + Vite 构建。

## 页面

| 路由 | 说明 |
|------|------|
| `/admin` | Dashboard — 系统信息、运行时间、集群状态 |
| `/admin/config` | 配置查看 — 当前运行配置（含集群参数） |
| `/admin/plugins` | 插件管理 — 已启用/可用插件列表 |
| `/admin/cluster` | 集群节点 — 节点列表、状态、角色 |

## 开发

```bash
npm install
npm run build
```

构建产物输出到 `dist/`，由 Rust 服务静态托管在 `/admin` 路径下。

## 技术栈

- Vue 3 (`<script setup>`)
- TypeScript
- Vite
- Axios
