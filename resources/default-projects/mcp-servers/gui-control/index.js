#!/usr/bin/env node

/**
 * MCP Server for zAI GUI control.
 *
 * Provides tools for supported AI coding assistants to control
 * the app's right panel: switch tabs, open/close the panel.
 *
 * Communication: reads the GUI server port from a file specified by
 * the ZAI_GUI_PORT_FILE environment variable, then connects via WebSocket.
 */

const { Server } = require('@modelcontextprotocol/sdk/server/index.js')
const { StdioServerTransport } = require('@modelcontextprotocol/sdk/server/stdio.js')
const {
  CallToolRequestSchema,
  ListToolsRequestSchema
} = require('@modelcontextprotocol/sdk/types.js')
const { readFileSync, writeFileSync } = require('fs')
const { join } = require('path')
const { WebSocket } = require('ws')

const VALID_TABS = ['tips', 'agents', 'skills', 'mcps', 'canvas']
const VALID_CANVAS_MODES = ['panel', 'full', 'bottom']

const MAX_HTML_SIZE = 1024 * 1024 // 1MB

/**
 * Returns the project path from env var or falls back to cwd.
 */
function getProjectPath() {
  return process.env.ZAI_PROJECT_PATH || process.env.YFT_PROJECT_PATH || process.cwd()
}

/**
 * Sends an action to the GUI server via WebSocket and waits for a response.
 */
function sendAction(port, action) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error('Connection to GUI server timed out'))
    }, 5000)

    let ws
    try {
      ws = new WebSocket(`ws://127.0.0.1:${port}`)
    } catch (err) {
      clearTimeout(timeout)
      reject(new Error(`Failed to create WebSocket connection: ${err.message}`))
      return
    }

    ws.on('open', () => {
      ws.send(JSON.stringify(action))
    })

    ws.on('message', (data) => {
      clearTimeout(timeout)
      try {
        const result = JSON.parse(data.toString())
        ws.close()
        resolve(result)
      } catch (err) {
        ws.close()
        reject(new Error(`Invalid response from GUI server: ${err.message}`))
      }
    })

    ws.on('error', (err) => {
      clearTimeout(timeout)
      reject(new Error(`WebSocket error: ${err.message}`))
    })
  })
}

/**
 * Reads the GUI server port from the port file.
 */
function readPort() {
  const portFile = process.env.ZAI_GUI_PORT_FILE || process.env.YFT_GUI_PORT_FILE
  if (!portFile) {
    throw new Error('ZAI_GUI_PORT_FILE environment variable is not set')
  }

  try {
    const content = readFileSync(portFile, 'utf-8').trim()
    const port = parseInt(content, 10)
    if (isNaN(port) || port <= 0) {
      throw new Error(`Invalid port number: "${content}"`)
    }
    return port
  } catch (err) {
    if (err.code === 'ENOENT') {
      throw new Error(
        `Port file not found at "${portFile}". Is zAI running?`
      )
    }
    throw err
  }
}

async function main() {
  const server = new Server(
    {
      name: 'gui-control',
      version: '1.0.0'
    },
    {
      capabilities: {
        tools: {}
      }
    }
  )

  server.setRequestHandler(ListToolsRequestSchema, async () => {
    return {
      tools: [
        {
          name: 'switch_tab',
          description:
            'Switch the right panel of zAI to a specific tab. Available tabs: tips (shows tips.md content), agents (lists AI agents), skills (lists available skills), mcps (shows MCP server configurations).',
          inputSchema: {
            type: 'object',
            properties: {
              tab: {
                type: 'string',
                enum: VALID_TABS,
                description: 'The tab to switch to'
              }
            },
            required: ['tab']
          }
        },
        {
          name: 'open_panel',
          description:
            'Open (expand) the right panel of zAI if it is currently collapsed.',
          inputSchema: {
            type: 'object',
            properties: {}
          }
        },
        {
          name: 'close_panel',
          description:
            'Close (collapse) the right panel of zAI.',
          inputSchema: {
            type: 'object',
            properties: {}
          }
        },
        {
          name: 'render_ui',
          description:
            'Build a project insight interface beside the terminal. Write a self-contained HTML document (inline CSS/JS) following Windows 11 light-theme design (light backgrounds, Segoe UI, subtle borders). The UI can read contained project files at runtime using relative paths with window.yft.readFile(path) and yft.readDir(path). Use mode "full" (replaces sidebar + right panel, primary), "bottom" (below terminal), or "panel" (right sidebar tab). Do NOT call this on every response — only when the user asks or an important insight warrants it. Max 1MB.',
          inputSchema: {
            type: 'object',
            properties: {
              html: {
                type: 'string',
                description: 'Complete HTML document (<!DOCTYPE html>...) to render'
              },
              mode: {
                type: 'string',
                enum: VALID_CANVAS_MODES,
                description: 'Layout mode: "panel" (Canvas tab in right panel, default), "full" (full-window overlay), "bottom" (horizontal split below terminal)'
              }
            },
            required: ['html']
          }
        },
        {
          name: 'get_ui',
          description:
            'Read the current canvas.html content from the project. Returns the HTML string or an error if the file does not exist. Use this before calling render_ui to make incremental updates.',
          inputSchema: {
            type: 'object',
            properties: {}
          }
        },
        {
          name: 'add_connection',
          description:
            'Add a new MCP connection to the current project in zAI. Supports SSE/remote (proxied via mcp-remote) and stdio/local connections. After adding, the Connections tab is auto-selected and the list refreshes.',
          inputSchema: {
            type: 'object',
            properties: {
              name: {
                type: 'string',
                description: 'Unique name for the connection (e.g. "my-api", "github-mcp")'
              },
              type: {
                type: 'string',
                enum: ['sse', 'stdio'],
                description: 'Connection type: "sse" for remote SSE/HTTP servers (URL will be proxied via mcp-remote), "stdio" for local command-based servers'
              },
              url: {
                type: 'string',
                description: 'For SSE type: the server URL (e.g. "https://api.example.com/mcp/sse"). For stdio type: the command to run (e.g. "npx -y @modelcontextprotocol/server-filesystem")'
              },
              headers: {
                type: 'object',
                additionalProperties: { type: 'string' },
                description: 'Optional key-value headers (for SSE) or environment variables (for stdio). Example: {"Authorization": "Bearer token123"}'
              }
            },
            required: ['name', 'type', 'url']
          }
        }
      ]
    }
  })

  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    const { name, arguments: args } = request.params

    // get_ui is a pure file read — no WebSocket needed
    if (name === 'get_ui') {
      try {
        const canvasPath = join(getProjectPath(), 'canvas.html')
        const content = readFileSync(canvasPath, 'utf-8')
        return { content: [{ type: 'text', text: content }] }
      } catch (err) {
        if (err.code === 'ENOENT') {
          return {
            content: [{ type: 'text', text: 'No canvas.html found in the project. Use render_ui to create one.' }],
            isError: true
          }
        }
        return {
          content: [{ type: 'text', text: `Error reading canvas.html: ${err.message}` }],
          isError: true
        }
      }
    }

    let port
    try {
      port = readPort()
    } catch (err) {
      return {
        content: [
          {
            type: 'text',
            text: `Error: ${err.message}`
          }
        ],
        isError: true
      }
    }

    let action
    switch (name) {
      case 'switch_tab': {
        const tab = args?.tab
        if (!tab || !VALID_TABS.includes(tab)) {
          return {
            content: [
              {
                type: 'text',
                text: `Error: Invalid tab "${tab}". Must be one of: ${VALID_TABS.join(', ')}`
              }
            ],
            isError: true
          }
        }
        action = { action: 'switch_tab', tab }
        break
      }

      case 'open_panel':
        action = { action: 'open_panel' }
        break

      case 'close_panel':
        action = { action: 'close_panel' }
        break

      case 'render_ui': {
        const html = args?.html
        const mode = args?.mode || 'panel'
        if (!html || typeof html !== 'string') {
          return {
            content: [{ type: 'text', text: 'Error: "html" is required and must be a string' }],
            isError: true
          }
        }
        if (html.length > MAX_HTML_SIZE) {
          return {
            content: [{ type: 'text', text: `Error: HTML exceeds maximum size of 1MB (got ${(html.length / 1024 / 1024).toFixed(2)}MB)` }],
            isError: true
          }
        }
        if (!VALID_CANVAS_MODES.includes(mode)) {
          return {
            content: [{ type: 'text', text: `Error: Invalid mode "${mode}". Must be one of: ${VALID_CANVAS_MODES.join(', ')}` }],
            isError: true
          }
        }
        try {
          const canvasPath = join(getProjectPath(), 'canvas.html')
          writeFileSync(canvasPath, html, 'utf-8')
        } catch (err) {
          return {
            content: [{ type: 'text', text: `Error writing canvas.html: ${err.message}` }],
            isError: true
          }
        }
        // Set canvas mode and show canvas
        if (mode === 'panel') {
          action = { action: 'switch_tab', tab: 'canvas' }
        } else {
          action = { action: 'set_canvas_mode', mode }
        }
        break
      }

      case 'add_connection': {
        const connName = args?.name
        const connType = args?.type
        const connUrl = args?.url
        const connHeaders = args?.headers || {}

        if (!connName || typeof connName !== 'string') {
          return {
            content: [{ type: 'text', text: 'Error: "name" is required and must be a string' }],
            isError: true
          }
        }
        if (!connType || !['sse', 'stdio'].includes(connType)) {
          return {
            content: [{ type: 'text', text: `Error: Invalid type "${connType}". Must be "sse" or "stdio"` }],
            isError: true
          }
        }
        if (!connUrl || typeof connUrl !== 'string') {
          return {
            content: [{ type: 'text', text: 'Error: "url" is required and must be a string' }],
            isError: true
          }
        }
        action = {
          action: 'add_connection',
          name: connName,
          type: connType,
          url: connUrl,
          headers: connHeaders
        }
        break
      }

      default:
        return {
          content: [
            {
              type: 'text',
              text: `Error: Unknown tool "${name}"`
            }
          ],
          isError: true
        }
    }

    try {
      const result = await sendAction(port, action)
      if (result.success) {
        return {
          content: [
            {
              type: 'text',
              text: `Successfully executed ${name}${args?.tab ? ` (tab: ${args.tab})` : ''}`
            }
          ]
        }
      } else {
        return {
          content: [
            {
              type: 'text',
              text: `Error: ${result.error || 'Unknown error'}`
            }
          ],
          isError: true
        }
      }
    } catch (err) {
      return {
        content: [
          {
            type: 'text',
            text: `Error: ${err.message}`
          }
        ],
        isError: true
      }
    }
  })

  const transport = new StdioServerTransport()
  await server.connect(transport)
}

main().catch((err) => {
  console.error('Fatal error:', err)
  process.exit(1)
})
