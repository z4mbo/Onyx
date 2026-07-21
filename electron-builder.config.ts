import type { Configuration } from 'electron-builder'

const config: Configuration = {
  appId: 'io.github.z4mbo.zai',
  productName: 'zAI',
  executableName: 'zAI',
  artifactName: '${productName}-${version}-${os}-${arch}.${ext}',
  directories: {
    buildResources: 'resources',
    output: 'dist'
  },
  files: [
    'out/**/*',
    'package.json'
  ],
  asar: true,
  asarUnpack: ['node_modules/node-pty/**/*'],
  npmRebuild: true,
  extraResources: [
    {
      from: 'resources/default-projects',
      to: 'default-projects',
      filter: ['**/*']
    },
    {
      from: 'resources/icon.ico',
      to: 'icon.ico'
    },
    {
      from: 'resources/logo.png',
      to: 'logo.png'
    },
    {
      from: 'LICENSE',
      to: 'LICENSE'
    },
    {
      from: 'ATTRIBUTION.md',
      to: 'ATTRIBUTION.md'
    }
  ],
  win: {
    target: [
      {
        target: 'nsis',
        arch: ['x64']
      }
    ],
    icon: 'resources/icon.ico',
    legalTrademarks: 'zAI'
  },
  nsis: {
    oneClick: false,
    allowToChangeInstallationDirectory: true,
    createDesktopShortcut: true,
    createStartMenuShortcut: true,
    shortcutName: 'zAI',
    installerIcon: 'resources/icon.ico',
    uninstallerIcon: 'resources/icon.ico',
    installerHeader: 'resources/installer/header.bmp',
    installerSidebar: 'resources/installer/sidebar.bmp',
    uninstallerSidebar: 'resources/installer/sidebar.bmp',
    license: 'LICENSE',
    include: 'resources/installer/installer.nsh'
  },
  mac: {
    target: ['dmg', 'zip'],
    icon: 'resources/logo.png',
    category: 'public.app-category.developer-tools',
    hardenedRuntime: true,
    gatekeeperAssess: false,
    entitlements: 'build/entitlements.mac.plist',
    entitlementsInherit: 'build/entitlements.mac.plist'
  },
  dmg: {
    title: 'zAI ${version}',
    artifactName: '${productName}-${version}-mac-${arch}.${ext}'
  }
}

export default config
