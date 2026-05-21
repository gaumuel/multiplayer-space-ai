const VERT = `#version 300 es
in vec3 a_position;
in vec4 a_color;
in float a_size;
in float a_type;

uniform mat4 u_projection;
uniform mat4 u_view;
uniform float u_time;

out vec4 v_color;
out float v_type;
out vec3 v_position;

void main() {
    v_color = a_color;
    v_type = a_type;
    v_position = a_position;
    
    vec4 clipPos = u_projection * u_view * vec4(a_position, 1.0);
    gl_Position = clipPos;
    gl_PointSize = a_size * (1500.0 / clipPos.w);
}`;

const FRAG = `#version 300 es
precision mediump float;

in vec4 v_color;
in float v_type;
in vec3 v_position;

out vec4 fragColor;

void main() {
    vec2 coord = gl_PointCoord * 2.0 - 1.0;
    float dist = length(coord);
    
    if (dist > 1.0) discard;
    
    vec3 color = v_color.rgb;
    float alpha = v_color.a;
    
    if (v_type < 0.5) {
        float glow = 1.0 - dist;
        color += vec3(0.1) * glow;
    } else if (v_type < 1.5) {
        float core = smoothstep(0.5, 0.0, dist);
        color += vec3(0.5, 0.5, 0.0) * core;
    } else if (v_type < 2.5) {
        float ring = smoothstep(0.8, 0.6, dist) - smoothstep(0.6, 0.4, dist);
        color += vec3(0.2) * ring;
        float inner = smoothstep(0.4, 0.0, dist);
        color = mix(color, vec3(0.1, 0.3, 0.5), inner);
    }
    
    float edge = 1.0 - smoothstep(0.7, 1.0, dist);
    alpha *= edge;
    
    fragColor = vec4(color, alpha);
}`;

const GRID_VERT = `#version 300 es
in vec3 a_position;
in vec4 a_color;

uniform mat4 u_projection;
uniform mat4 u_view;

out vec4 v_color;

void main() {
    v_color = a_color;
    vec4 clipPos = u_projection * u_view * vec4(a_position, 1.0);
    gl_Position = clipPos;
    gl_PointSize = max(1.0, 50.0 / clipPos.w);
}`;

const GRID_FRAG = `#version 300 es
precision mediump float;

in vec4 v_color;
out vec4 fragColor;

void main() {
    fragColor = v_color;
}`;

import type { mat4 } from 'gl-matrix';

export class WebGLRenderer {
  private gl: WebGL2RenderingContext;
  private program: WebGLProgram;
  private gridProgram: WebGLProgram;
  private vao: WebGLVertexArrayObject;
  private gridVao: WebGLVertexArrayObject;
  private posBuffer: WebGLBuffer;
  private colorBuffer: WebGLBuffer;
  private sizeBuffer: WebGLBuffer;
  private typeBuffer: WebGLBuffer;
  private gridPosBuffer: WebGLBuffer;
  private gridColorBuffer: WebGLBuffer;
  private gridPositions!: Float32Array;
  private gridColors!: Float32Array;
  private gridVertexCount = 0;

  private uProjection: WebGLUniformLocation;
  private uView: WebGLUniformLocation;
  private uTime: WebGLUniformLocation;
  private uGridProjection: WebGLUniformLocation;
  private uGridView: WebGLUniformLocation;

  private maxEntities = 100000;
  private positions: Float32Array;
  private colors: Float32Array;
  private sizes: Float32Array;
  private types: Float32Array;

  constructor(canvas: HTMLCanvasElement) {
    const gl = canvas.getContext('webgl2', { antialias: true, alpha: false });
    if (!gl) throw new Error('WebGL2 not supported');
    this.gl = gl;

    this.program = this.createProgram(VERT, FRAG);
    this.gridProgram = this.createProgram(GRID_VERT, GRID_FRAG);

    this.uProjection = gl.getUniformLocation(this.program, 'u_projection')!;
    this.uView = gl.getUniformLocation(this.program, 'u_view')!;
    this.uTime = gl.getUniformLocation(this.program, 'u_time')!;
    this.uGridProjection = gl.getUniformLocation(this.gridProgram, 'u_projection')!;
    this.uGridView = gl.getUniformLocation(this.gridProgram, 'u_view')!;

    this.positions = new Float32Array(this.maxEntities * 3);
    this.colors = new Float32Array(this.maxEntities * 4);
    this.sizes = new Float32Array(this.maxEntities);
    this.types = new Float32Array(this.maxEntities);

    this.vao = gl.createVertexArray()!;
    gl.bindVertexArray(this.vao);

    this.posBuffer = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.posBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, this.positions.byteLength, gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 0, 0);

    this.colorBuffer = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.colorBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, this.colors.byteLength, gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 4, gl.FLOAT, false, 0, 0);

    this.sizeBuffer = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.sizeBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, this.sizes.byteLength, gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribPointer(2, 1, gl.FLOAT, false, 0, 0);

    this.typeBuffer = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.typeBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, this.types.byteLength, gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(3);
    gl.vertexAttribPointer(3, 1, gl.FLOAT, false, 0, 0);

    this.gridVao = gl.createVertexArray()!;
    gl.bindVertexArray(this.gridVao);

    this.gridPosBuffer = gl.createBuffer()!;
    this.gridColorBuffer = gl.createBuffer()!;

    this.buildGrid();

    gl.bindVertexArray(null);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.enable(gl.DEPTH_TEST);
    gl.depthFunc(gl.LEQUAL);
  }

  private buildGrid() {
    const gridSize = 40;
    const spacing = 150;
    const count = (gridSize * 2 + 1) ** 2;
    this.gridPositions = new Float32Array(count * 3);
    this.gridColors = new Float32Array(count * 4);
    this.gridVertexCount = count;

    let idx = 0;
    for (let x = -gridSize; x <= gridSize; x++) {
      for (let y = -gridSize; y <= gridSize; y++) {
        this.gridPositions[idx * 3] = x * spacing;
        this.gridPositions[idx * 3 + 1] = y * spacing;
        this.gridPositions[idx * 3 + 2] = 0;
        this.gridColors[idx * 4] = 0.15;
        this.gridColors[idx * 4 + 1] = 0.15;
        this.gridColors[idx * 4 + 2] = 0.15;
        this.gridColors[idx * 4 + 3] = 1.0;
        idx++;
      }
    }

    const gl = this.gl;
    gl.bindVertexArray(this.gridVao);

    gl.bindBuffer(gl.ARRAY_BUFFER, this.gridPosBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, this.gridPositions, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 0, 0);

    gl.bindBuffer(gl.ARRAY_BUFFER, this.gridColorBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, this.gridColors, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 4, gl.FLOAT, false, 0, 0);

    gl.bindVertexArray(null);
  }

  private createProgram(vs: string, fs: string): WebGLProgram {
    const gl = this.gl;
    const vShader = this.compileShader(gl.VERTEX_SHADER, vs);
    const fShader = this.compileShader(gl.FRAGMENT_SHADER, fs);
    const program = gl.createProgram()!;
    gl.attachShader(program, vShader);
    gl.attachShader(program, fShader);
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      throw new Error(gl.getProgramInfoLog(program) ?? 'Program link failed');
    }
    return program;
  }

  private compileShader(type: number, src: string): WebGLShader {
    const gl = this.gl;
    const shader = gl.createShader(type)!;
    gl.shaderSource(shader, src);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      throw new Error(gl.getShaderInfoLog(shader) ?? 'Shader compile failed');
    }
    return shader;
  }

  render(
    positions: Float32Array,
    colors: Float32Array,
    sizes: Float32Array,
    types: Float32Array,
    count: number,
    viewMatrix: mat4,
    projectionMatrix: mat4,
    time: number,
    rects?: { x: number; y: number; w: number; h: number; r: number; g: number; b: number }[],
  ) {
    const gl = this.gl;
    gl.viewport(0, 0, gl.canvas.width, gl.canvas.height);
    gl.clearColor(0.008, 0.016, 0.047, 1.0);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);

    gl.useProgram(this.gridProgram);
    gl.uniformMatrix4fv(this.uGridProjection, false, new Float32Array(projectionMatrix));
    gl.uniformMatrix4fv(this.uGridView, false, new Float32Array(viewMatrix));
    gl.bindVertexArray(this.gridVao);
    gl.drawArrays(gl.POINTS, 0, this.gridVertexCount);

    // Draw rectangles (obstacles) using the grid shader (it handles position + color)
    if (rects && rects.length > 0) {
      this.renderRects(rects, viewMatrix, projectionMatrix);
    }

    if (count > 0) {
      gl.useProgram(this.program);
      gl.uniformMatrix4fv(this.uProjection, false, new Float32Array(projectionMatrix));
      gl.uniformMatrix4fv(this.uView, false, new Float32Array(viewMatrix));
      gl.uniform1f(this.uTime, time);

      gl.bindVertexArray(this.vao);

      gl.bindBuffer(gl.ARRAY_BUFFER, this.posBuffer);
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, positions.subarray(0, count * 3));

      gl.bindBuffer(gl.ARRAY_BUFFER, this.colorBuffer);
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, colors.subarray(0, count * 4));

      gl.bindBuffer(gl.ARRAY_BUFFER, this.sizeBuffer);
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, sizes.subarray(0, count));

      gl.bindBuffer(gl.ARRAY_BUFFER, this.typeBuffer);
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, types.subarray(0, count));

      gl.drawArrays(gl.POINTS, 0, count);
    }
  }

  private renderRects(rects: { x: number; y: number; w: number; h: number; r: number; g: number; b: number }[], viewMatrix: mat4, projectionMatrix: mat4) {
    const gl = this.gl;
    // 6 vertices per rect (2 triangles)
    const vertCount = rects.length * 6;
    const positions = new Float32Array(vertCount * 3);
    const colors = new Float32Array(vertCount * 4);

    let idx = 0;
    for (const rect of rects) {
      const x0 = rect.x - rect.w / 2;
      const x1 = rect.x + rect.w / 2;
      const y0 = rect.y - rect.h / 2;
      const y1 = rect.y + rect.h / 2;
      const z = 5.0;

      // Triangle 1
      positions[idx * 3] = x0; positions[idx * 3 + 1] = y0; positions[idx * 3 + 2] = z; idx++;
      positions[idx * 3] = x1; positions[idx * 3 + 1] = y0; positions[idx * 3 + 2] = z; idx++;
      positions[idx * 3] = x1; positions[idx * 3 + 1] = y1; positions[idx * 3 + 2] = z; idx++;
      // Triangle 2
      positions[idx * 3] = x0; positions[idx * 3 + 1] = y0; positions[idx * 3 + 2] = z; idx++;
      positions[idx * 3] = x1; positions[idx * 3 + 1] = y1; positions[idx * 3 + 2] = z; idx++;
      positions[idx * 3] = x0; positions[idx * 3 + 1] = y1; positions[idx * 3 + 2] = z; idx++;
    }

    // Fill colors
    idx = 0;
    for (const rect of rects) {
      for (let v = 0; v < 6; v++) {
        colors[idx * 4] = rect.r;
        colors[idx * 4 + 1] = rect.g;
        colors[idx * 4 + 2] = rect.b;
        colors[idx * 4 + 3] = 0.8;
        idx++;
      }
    }

    gl.useProgram(this.gridProgram);
    gl.uniformMatrix4fv(this.uGridProjection, false, new Float32Array(projectionMatrix));
    gl.uniformMatrix4fv(this.uGridView, false, new Float32Array(viewMatrix));
    gl.bindVertexArray(this.gridVao);

    // Reuse grid buffers temporarily
    gl.bindBuffer(gl.ARRAY_BUFFER, this.gridPosBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, positions, gl.DYNAMIC_DRAW);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.gridColorBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, colors, gl.DYNAMIC_DRAW);

    gl.drawArrays(gl.TRIANGLES, 0, vertCount);

    // Restore grid data
    this.buildGrid();
  }
}
