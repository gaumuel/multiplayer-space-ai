import { Snapshot } from '../types';

export class WtClient {
  private connection: WebTransport | null = null;
  private onSnapshot: ((snapshot: Snapshot) => void) | null = null;
  private running = false;

  constructor(
    private url: string,
  ) {}

  setOnSnapshot(handler: (snapshot: Snapshot) => void) {
    this.onSnapshot = handler;
  }

  async connect() {
    // Fetch the server's self-signed cert hash
    const resp = await fetch('http://localhost:4434/cert-hash');
    const { hash } = await resp.json();
    const hashBytes = Uint8Array.from(atob(hash), c => c.charCodeAt(0));

    this.connection = new WebTransport(this.url, {
      serverCertificateHashes: [{
        algorithm: 'sha-256',
        value: hashBytes.buffer,
      }],
    });
    await this.connection.ready;
    this.running = true;
    this.listen();
  }

  private async listen() {
    if (!this.connection) return;

    try {
      const reader = this.connection.incomingUnidirectionalStreams.getReader();

      while (this.running) {
        const { value: stream, done } = await reader.read();
        if (done) break;

        const streamReader = stream.getReader();
        const chunks: Uint8Array[] = [];

        while (true) {
          const { value, done } = await streamReader.read();
          if (done) break;
          chunks.push(value);
        }

        const totalLen = chunks.reduce((sum, c) => sum + c.length, 0);
        const data = new Uint8Array(totalLen);
        let offset = 0;
        for (const chunk of chunks) {
          data.set(chunk, offset);
          offset += chunk.length;
        }

        // Skip 4-byte length prefix
        const payload = data.slice(4);

        if (payload.length > 0 && this.onSnapshot) {
          const snapshot = this.decodeSnapshot(payload);
          if (snapshot) {
            this.onSnapshot(snapshot);
          }
        }
      }
    } catch (e) {
      console.error('WebTransport stream read error:', e);
    }
  }

  private decodeSnapshot(data: Uint8Array): Snapshot | null {
    try {
      const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
      let offset = 0;

      const tick = view.getUint32(offset, true);
      offset += 4;

      const entityCount = view.getUint32(offset, true);
      offset += 4;

      const entities: Snapshot['entities'] = [];

      for (let i = 0; i < entityCount; i++) {
        if (offset + 18 > data.byteLength) break;

        const id = view.getUint32(offset, true);
        offset += 4;

        const x = view.getFloat32(offset, true);
        offset += 4;

        const y = view.getFloat32(offset, true);
        offset += 4;

        const z = view.getFloat32(offset, true);
        offset += 4;

        const entityType = view.getUint8(offset);
        offset += 1;

        const teamFlag = view.getUint8(offset);
        offset += 1;
        const team = teamFlag === 0xFF ? null : teamFlag;

        const healthFlag = view.getUint8(offset);
        offset += 1;
        let health: number | null = null;
        if (healthFlag === 1) {
          health = view.getFloat32(offset, true);
          offset += 4;
        }

        const maxHealthFlag = view.getUint8(offset);
        offset += 1;
        let maxHealth: number | null = null;
        if (maxHealthFlag === 1) {
          maxHealth = view.getFloat32(offset, true);
          offset += 4;
        }

        entities.push({
          id,
          x,
          y,
          z,
          entity_type: entityType,
          team,
          health,
          max_health: maxHealth,
        });
      }

      return { tick, entities };
    } catch {
      return null;
    }
  }

  async disconnect() {
    this.running = false;
    if (this.connection) {
      await this.connection.close();
      this.connection = null;
    }
  }
}
