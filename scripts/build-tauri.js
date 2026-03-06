import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const rootDir = path.resolve(__dirname, '..')

const npmCmd = process.platform === 'win32' ? 'npm.cmd' : 'npm'

function run(cwd, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(npmCmd, args, {
      cwd,
      stdio: 'inherit',
      env: process.env,
      shell: true,
    })

    child.on('exit', (code) => {
      if (code === 0) resolve()
      else reject(new Error(`${args.join(' ')} 失败，退出码 ${code}`))
    })
  })
}

async function main() {
  await run(rootDir, ['run', 'build'])
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
