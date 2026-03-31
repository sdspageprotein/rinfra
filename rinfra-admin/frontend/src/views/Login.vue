<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { getInfo } from '../api'

const token = ref('')
const error = ref('')
const loading = ref(false)
const router = useRouter()

async function handleLogin() {
  const val = token.value.trim()
  if (!val) {
    error.value = 'Please enter a token'
    return
  }

  loading.value = true
  error.value = ''
  localStorage.setItem('admin_token', val)

  try {
    await getInfo()
    router.push('/')
  } catch {
    localStorage.removeItem('admin_token')
    error.value = 'Invalid token'
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="login-page">
    <div class="login-card">
      <h1>rinfra</h1>
      <p class="subtitle">Admin Panel</p>
      <form @submit.prevent="handleLogin">
        <input
          v-model="token"
          type="password"
          placeholder="Enter admin token"
          class="token-input"
          autofocus
        />
        <p v-if="error" class="error">{{ error }}</p>
        <button type="submit" class="login-btn" :disabled="loading">
          {{ loading ? 'Verifying...' : 'Login' }}
        </button>
      </form>
    </div>
  </div>
</template>

<style scoped>
.login-page {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
}

.login-card {
  background: #fff;
  border-radius: 16px;
  padding: 48px 40px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
  width: 380px;
  text-align: center;
}

h1 {
  margin: 0 0 4px;
  font-size: 2rem;
  color: #7c83ff;
}

.subtitle {
  color: #888;
  font-size: 0.85rem;
  text-transform: uppercase;
  letter-spacing: 2px;
  margin-bottom: 32px;
}

.token-input {
  width: 100%;
  padding: 12px 16px;
  border: 2px solid #e0e0e0;
  border-radius: 8px;
  font-size: 0.95rem;
  outline: none;
  transition: border-color 0.2s;
  box-sizing: border-box;
}

.token-input:focus {
  border-color: #7c83ff;
}

.error {
  color: #e53935;
  font-size: 0.85rem;
  margin: 8px 0 0;
}

.login-btn {
  width: 100%;
  margin-top: 20px;
  padding: 12px;
  background: #7c83ff;
  color: #fff;
  border: none;
  border-radius: 8px;
  font-size: 1rem;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.2s;
}

.login-btn:hover:not(:disabled) {
  background: #5a60e0;
}

.login-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
</style>
