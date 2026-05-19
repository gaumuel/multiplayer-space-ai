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
    this.connection = new WebTransport(this.url);
    await this.connection.ready;
    this.running = true;
    this.listen();
  }

  private async listen() {
    if (!this.connection) return;

    const reader = this.connection.datagrams.readable.getReader();

    try {
      while (this.running) {
        const { value, done } = await reader.read();
        if (done) break;

        if (value && this.onSnapshot) {
          const snapshot = this.decodeSnapshot(value);
          if (snapshot) {
            this.onSnapshot(snapshot);
          }
        }
      }
    } catch (e) {
      console.error('WebTransport datagram read error:', e);
    } finally {
      reader.releaseLock();
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
        if (offset + 32 > data.byteLength) break;

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
