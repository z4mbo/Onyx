const CANVAS_BRIDGE_PREAMBLE = `<script>
(function() {
  var requestId = 0;
  var pending = {};
  window.addEventListener('message', function(event) {
    if (event.data && event.data.type === 'yft_response' && pending[event.data.reqId]) {
      pending[event.data.reqId](event.data);
      delete pending[event.data.reqId];
    }
  });
  function request(type, payload) {
    return new Promise(function(resolve) {
      var id = ++requestId;
      pending[id] = resolve;
      parent.postMessage(Object.assign({ type: type, reqId: id }, payload), '*');
      setTimeout(function() {
        if (pending[id]) {
          pending[id]({ error: 'timeout' });
          delete pending[id];
        }
      }, 5000);
    });
  }
  var bridge = {
    sendToTerminal: function(text) {
      parent.postMessage({ type: 'send_to_terminal', text: String(text) }, '*');
    },
    switchTab: function(tab) {
      parent.postMessage({ type: 'switch_tab', tab: String(tab) }, '*');
    },
    setMode: function(mode) {
      parent.postMessage({ type: 'set_canvas_mode', mode: String(mode) }, '*');
    },
    readFile: function(path) {
      return request('read_file', { path: String(path) }).then(function(response) {
        return response.error ? null : response.content;
      });
    },
    readDir: function(path) {
      return request('read_dir', { path: String(path) }).then(function(response) {
        return response.error ? null : response.entries;
      });
    }
  };
  window.zai = bridge;
  window.yft = bridge;
})();
</script>`

/** Injects the fixed compatibility bridge into project-authored Canvas HTML. */
export function buildCanvasDocument(content: string): string {
  const head = /<head\b[^>]*>/i.exec(content)
  if (head?.index !== undefined) {
    const insertionPoint = head.index + head[0].length
    return content.slice(0, insertionPoint) + CANVAS_BRIDGE_PREAMBLE + content.slice(insertionPoint)
  }

  const body = /<body\b[^>]*>/i.exec(content)
  if (body?.index !== undefined) {
    const insertionPoint = body.index + body[0].length
    return content.slice(0, insertionPoint) + CANVAS_BRIDGE_PREAMBLE + content.slice(insertionPoint)
  }

  const doctype = /<!doctype\b[^>]*>/i.exec(content)
  if (doctype?.index !== undefined) {
    const insertionPoint = doctype.index + doctype[0].length
    return content.slice(0, insertionPoint) + CANVAS_BRIDGE_PREAMBLE + content.slice(insertionPoint)
  }

  return CANVAS_BRIDGE_PREAMBLE + content
}
