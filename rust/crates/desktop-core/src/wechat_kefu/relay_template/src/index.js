// ClaudeWiki WeChat Customer Service Relay Worker
// Receives WeChat HTTP callbacks and relays them via WebSocket to the desktop app.

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === '/health') return new Response('ok');
    if (url.pathname === '/callback' || url.pathname === '/ws') {
      const id = env.RELAY.idFromName('default');
      return env.RELAY.get(id).fetch(request);
    }
    return new Response('not found', { status: 404 });
  }
};

export class RelayDO {
  constructor(state, env) {
    this.state = state;
    this.env = env;
    this.buffer = [];
  }

  async fetch(request) {
    const url = new URL(request.url);

    // Desktop WebSocket connection
    if (url.pathname === '/ws') {
      if (request.headers.get('Upgrade') !== 'websocket') {
        return new Response('expected websocket', { status: 426 });
      }
      if (url.searchParams.get('auth') !== this.env.AUTH_TOKEN) {
        return new Response('unauthorized', { status: 401 });
      }
      const pair = new WebSocketPair();
      this.state.acceptWebSocket(pair[1]);
      for (const msg of this.buffer) pair[1].send(msg);
      this.buffer = [];
      return new Response(null, { status: 101, webSocket: pair[0] });
    }

    // GET /callback — WeChat echostr verification
    if (request.method === 'GET' && url.pathname === '/callback') {
      return this.handleVerify(url.searchParams);
    }

    // POST /callback — WeChat event notification → relay via WebSocket
    if (request.method === 'POST' && url.pathname === '/callback') {
      const body = await request.text();
      const relay = JSON.stringify({
        type: 'callback',
        params: url.search,
        body,
        ts: Date.now()
      });
      const clients = this.state.getWebSockets();
      if (clients.length > 0) {
        for (const ws of clients) ws.send(relay);
      } else {
        this.buffer.push(relay);
        if (this.buffer.length > 100) this.buffer.shift();
      }
      return new Response('success');
    }

    return new Response('not found', { status: 404 });
  }

  async handleVerify(params) {
    const msgSig = params.get('msg_signature') || '';
    const timestamp = params.get('timestamp') || '';
    const nonce = params.get('nonce') || '';
    const echostr = params.get('echostr') || '';

    // SHA1 signature verification
    const token = this.env.CALLBACK_TOKEN;
    const parts = [token, timestamp, nonce, echostr].sort();
    const hash = await sha1(parts.join(''));
    if (hash !== msgSig) {
      return new Response('signature mismatch', { status: 403 });
    }

    // AES-256-CBC decrypt echostr
    const plaintext = await this.decryptEchostr(echostr);
    return new Response(plaintext);
  }

  async decryptEchostr(cipherB64) {
    const aesKeyB64 = this.env.ENCODING_AES_KEY + '=';
    const keyBytes = Uint8Array.from(atob(aesKeyB64), c => c.charCodeAt(0));
    const iv = keyBytes.slice(0, 16);

    const key = await crypto.subtle.importKey(
      'raw', keyBytes, { name: 'AES-CBC' }, false, ['decrypt']
    );
    const cipherBytes = Uint8Array.from(atob(cipherB64), c => c.charCodeAt(0));
    const plainBuf = await crypto.subtle.decrypt(
      { name: 'AES-CBC', iv }, key, cipherBytes
    );
    const plain = new Uint8Array(plainBuf);

    // Skip 16-byte random + read 4-byte msg length + extract message
    const msgLen = (plain[16] << 24) | (plain[17] << 16) | (plain[18] << 8) | plain[19];
    return new TextDecoder().decode(plain.slice(20, 20 + msgLen));
  }

  webSocketMessage(ws, msg) {
    try {
      const data = JSON.parse(msg);
      if (data.type === 'ping') {
        ws.send(JSON.stringify({ type: 'pong', ts: Date.now() }));
      }
    } catch {}
  }

  webSocketClose() {}
  webSocketError() {}
}

async function sha1(data) {
  const buf = await crypto.subtle.digest(
    'SHA-1', new TextEncoder().encode(data)
  );
  return [...new Uint8Array(buf)].map(b => b.toString(16).padStart(2, '0')).join('');
}
