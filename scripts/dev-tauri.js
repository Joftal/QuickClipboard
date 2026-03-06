import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import net from 'node:net'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const rootDir = path.resolve(__dirname, '..')

const isWin = process.platform === 'win32'
const devPort = Number(process.env.TAURI_DEV_PORT || 1421)
const devUrls = [
  `http://127.0.0.1:${devPort}`,
  `http://localhost:${devPort}`,
  `http://[::1]:${devPort}`,
]

function run(cwd, commandLine) {
  const child = isWin
    ? spawn('cmd.exe', ['/d', '/s', '/c', commandLine], {
        cwd,
        stdio: 'inherit',
        env: process.env,
      })
    : spawn('sh', ['-lc', commandLine], {
        cwd,
        stdio: 'inherit',
        env: process.env,
      })

  child.on('error', (err) => {
    console.error(err)
    process.exit(1)
  })

  child.on('exit', (code) => {
    if (code && code !== 0) {
      process.exit(code)
    }
  })

  return child
}

let host = null
let keepAliveTimer = null

function isPortInUseOnHost(port, host, timeoutMs = 1200) {
  return new Promise((resolve) => {
    const socket = new net.Socket()
    let finished = false

    const finish = (inUse) => {
      if (finished) return
      finished = true
      socket.destroy()
      resolve(inUse)
    }

    socket.setTimeout(timeoutMs)
    socket.once('connect', () => finish(true))
    socket.once('timeout', () => finish(false))
    socket.once('error', () => finish(false))
    socket.connect(port, host)
  })
}

async function isPortInUse(port) {
  const checks = await Promise.all([
    isPortInUseOnHost(port, '127.0.0.1'),
    isPortInUseOnHost(port, '::1'),
    isPortInUseOnHost(port, 'localhost'),
  ])
  return checks.some(Boolean)
}

async function detectViteDevServer(urls) {
  for (const url of urls) {
    try {
      const response = await fetch(`${url}/@vite/client`, {
        headers: {
          Accept: 'text/javascript,application/javascript,*/*',
        },
      })
      if (!response.ok) continue
      const content = await response.text()
      const isVite =
        content.includes('createHotContext') ||
        content.includes('updateStyle') ||
        content.includes('import.meta.hot')
      if (isVite) {
        return url
      }
    } catch {
      // try next candidate URL
    }
  }
  return null
}

function holdProcess() {
  if (keepAliveTimer) return
  keepAliveTimer = setInterval(() => {}, 60_000)
}

async function main() {
  const inUse = await isPortInUse(devPort)
  if (!inUse) {
    host = run(rootDir, `npm run dev -- --port ${devPort}`)
    return
  }

  const activeViteUrl = await detectViteDevServer(devUrls)
  if (!activeViteUrl) {
    console.error(
      `[tauri dev] Port ${devPort} is in use, but it is not serving a Vite dev server.`
    )
    console.error('[tauri dev] Stop the occupying process or set TAURI_DEV_PORT to another port.')
    process.exit(1)
  }

  console.log(`[tauri dev] Reusing existing Vite dev server: ${activeViteUrl}`)
  holdProcess()
}

function shutdown() {
  if (keepAliveTimer) {
    clearInterval(keepAliveTimer)
    keepAliveTimer = null
  }
  host?.kill()
}

process.on('SIGINT', () => {
  shutdown()
  process.exit(0)
})
process.on('SIGTERM', () => {
  shutdown()
  process.exit(0)
})

main().catch((err) => {
  console.error(err)
  shutdown()
  process.exit(1)
})
