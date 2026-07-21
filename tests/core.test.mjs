import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'

import { getCommand } from '../src/main/ai-engines/command-dictionary.ts'
import {
  getKimiVersionInvocation,
  isVersionAtLeast,
  parseSemanticVersion
} from '../src/main/ai-engines/kimi-version.ts'
import {
  buildKimiModelEnvironment,
  parseModelsPayload
} from '../src/main/openrouter/model-catalog.ts'
import {
  isRendererSettingKey,
  isValidRendererSettingValue
} from '../src/main/settings/settings-policy.ts'
import {
  resolveManagedProjectPath,
  validateProjectName
} from '../src/main/project/project-name.ts'
import { resolvePathWithinRoot } from '../src/main/filesystem/path-containment.ts'
import { buildCanvasDocument } from '../src/main/canvas/canvas-document.ts'

test('Kimi and OpenRouter commands use current Kimi Code syntax', () => {
  assert.equal(getCommand('kimi', 'add-dir', { dirPath: 'src' }), '/add-dir src')
  assert.equal(getCommand('kimi', 'add-file', { filePath: 'README.md' }), '@README.md')
  assert.equal(getCommand('kimi', 'continue-session'), 'kimi --continue')
  assert.equal(getCommand('openrouter', 'start-session'), 'kimi; exit')
  assert.equal(getCommand('openrouter', 'continue-session'), 'kimi --continue; exit')
})

test('Kimi provider compatibility requires version 0.6.0 or newer', () => {
  assert.deepEqual(parseSemanticVersion('kimi, version 0.6.0'), [0, 6, 0])
  assert.deepEqual(parseSemanticVersion('Kimi Code 1.2.3-beta.1'), [1, 2, 3])
  assert.equal(parseSemanticVersion('unknown'), null)
  assert.equal(isVersionAtLeast([0, 5, 9], [0, 6, 0]), false)
  assert.equal(isVersionAtLeast([0, 6, 0], [0, 6, 0]), true)
  assert.equal(isVersionAtLeast([1, 0, 0], [0, 6, 0]), true)
})

test('Kimi version detection safely launches Windows npm command shims', () => {
  const windowsInvocation = getKimiVersionInvocation(
    "C:\\Users\\A & B's PC\\AppData\\Roaming\\npm\\kimi.cmd",
    'win32'
  )
  assert.match(windowsInvocation.executable, /Windows[\\/]System32[\\/]WindowsPowerShell[\\/]v1\.0[\\/]powershell\.exe$/i)
  assert.deepEqual(windowsInvocation.args.slice(0, -1), [
    '-NoLogo',
    '-NoProfile',
    '-NonInteractive',
    '-EncodedCommand'
  ])
  assert.equal(
    Buffer.from(windowsInvocation.args.at(-1), 'base64').toString('utf16le'),
    "& 'C:\\Users\\A & B''s PC\\AppData\\Roaming\\npm\\kimi.cmd' --version; exit $LASTEXITCODE"
  )
  assert.deepEqual(getKimiVersionInvocation('/usr/local/bin/kimi', 'darwin'), {
    executable: '/usr/local/bin/kimi',
    args: ['--version']
  })
})

test('OpenRouter catalog keeps only sanitized tool-capable models', () => {
  const models = parseModelsPayload({
    data: [
      {
        id: 'vendor/text-only',
        name: 'No tools',
        context_length: 8192,
        supported_parameters: []
      },
      {
        id: 'vendor/vision-thinking',
        name: ' Vision\u0000 Model ',
        description: '  safe\n description ',
        context_length: 131072,
        supported_parameters: ['tools', 'reasoning'],
        architecture: { input_modalities: ['text', 'image'] }
      },
      {
        id: '../invalid id',
        context_length: 4096,
        supported_parameters: ['tools']
      }
    ]
  })

  assert.equal(models.length, 1)
  assert.equal(models[0].id, 'vendor/vision-thinking')
  assert.equal(models[0].name, 'Vision Model')
  assert.equal(models[0].supportsThinking, true)
  assert.equal(models[0].supportsImages, true)

  const env = buildKimiModelEnvironment('secret-value', models[0])
  assert.deepEqual(env, {
    KIMI_MODEL_PROVIDER_TYPE: 'openai',
    KIMI_MODEL_BASE_URL: 'https://openrouter.ai/api/v1',
    KIMI_MODEL_API_KEY: 'secret-value',
    KIMI_MODEL_NAME: 'vendor/vision-thinking',
    KIMI_MODEL_MAX_CONTEXT_SIZE: '131072',
    KIMI_MODEL_CAPABILITIES: 'tool_use,thinking,image_in'
  })
})

test('renderer settings policy exposes only validated non-secret settings', () => {
  assert.equal(isRendererSettingKey('defaultEngine'), true)
  assert.equal(isRendererSettingKey('openRouterApiKeyEncrypted'), false)
  assert.equal(isRendererSettingKey(undefined), false)
  assert.equal(isValidRendererSettingValue('defaultEngine', 'openrouter'), true)
  assert.equal(isValidRendererSettingValue('defaultEngine', 'unknown'), false)
  assert.equal(isValidRendererSettingValue('sidebarWidth', 280), true)
  assert.equal(isValidRendererSettingValue('sidebarWidth', '../escape'), false)
  assert.equal(isValidRendererSettingValue('terminalThemeCustom', { background: '#000000' }), true)
})

test('managed project paths cannot escape their root', () => {
  assert.equal(validateProjectName(' My Project '), 'My Project')
  assert.equal(resolveManagedProjectPath('/safe/projects', 'zAI'), resolve('/safe/projects', 'zAI'))
  for (const invalid of ['..', '../outside', 'nested/project', 'CON', 'trailing.']) {
    assert.throws(() => resolveManagedProjectPath('/safe/projects', invalid))
  }
})

test('Canvas paths are relative and contained within the active project', () => {
  assert.equal(
    resolvePathWithinRoot('/safe/project', 'src/index.ts'),
    resolve('/safe/project', 'src/index.ts')
  )
  for (const invalid of ['../secret', '/etc/passwd', 'C:\\Users\\secret.txt', 'nested/../../secret']) {
    assert.throws(() => resolvePathWithinRoot('/safe/project', invalid))
  }
})

test('Canvas documents receive the compatibility bridge without changing renderer CSP', () => {
  const document = buildCanvasDocument('<!doctype html><html><head><title>Test</title></head><body></body></html>')
  assert.ok(document.indexOf('window.zai = bridge') > document.indexOf('<head>'))
  assert.ok(document.indexOf('window.yft = bridge') < document.indexOf('<title>'))
  assert.match(document, /parent\.postMessage/)
})
