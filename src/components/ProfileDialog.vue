<script setup lang="ts">
import { reactive, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import Icon from "./Icon.vue";
import { saveProfile, toast } from "../store";
import { emptyProfile, type Profile } from "../types";

const props = defineProps<{ profile: Profile | null }>();
const emit = defineEmits<{ (e: "close"): void }>();

const form = reactive<Profile>({ ...emptyProfile(), ...(props.profile ?? {}) });
// 仅证书模式使用：私钥口令
const password = ref("");
const busy = ref(false);

const isNew = !props.profile?.id;

async function browseCert() {
  const p = await open({
    multiple: false,
    title: "选择客户端证书",
    filters: [{ name: "证书", extensions: ["pem", "crt", "cer"] }],
  });
  if (typeof p === "string") form.certificatePath = p;
}

async function browseKey() {
  const p = await open({
    multiple: false,
    title: "选择私钥文件",
    filters: [{ name: "私钥", extensions: ["pem", "key"] }],
  });
  if (typeof p === "string") form.keyPath = p;
}

async function browseBinary() {
  const p = await open({ multiple: false, title: "选择 openconnect 可执行文件" });
  if (typeof p === "string") form.openconnectPath = p;
}

async function submit() {
  if (busy.value) return;
  if (!form.name.trim()) {
    toast("请填写配置名称", "warn");
    return;
  }
  if (!form.gateway.trim()) {
    toast("请填写网关地址", "warn");
    return;
  }
  busy.value = true;
  try {
    const saved = await saveProfile({ ...form });
    if (saved) emit("close");
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="backdrop" @click.self="emit('close')">
    <div class="modal wide">
      <div class="modal-head">
        <Icon name="edit" :size="16" />
        <span>{{ isNew ? "新建配置" : "编辑配置" }}</span>
      </div>

      <div class="modal-body">
        <div class="grid-2">
          <label class="field">
            <span class="label">配置名称 *</span>
            <input v-model="form.name" type="text" placeholder="例如：总部 VPN" />
          </label>
          <label class="field">
            <span class="label">网关地址 *</span>
            <input v-model="form.gateway" type="text" placeholder="vpn.example.com" spellcheck="false" />
          </label>
        </div>

        <div class="grid-2">
          <label class="field">
            <span class="label">用户名</span>
            <input v-model="form.username" type="text" placeholder="登录账号" spellcheck="false" />
          </label>
          <label class="field">
            <span class="label">认证方式</span>
            <select v-model="form.authMethod">
              <option value="password">用户名 / 密码</option>
              <option value="certificate">客户端证书</option>
            </select>
          </label>
        </div>

        <label class="field">
          <span class="label">Group（可选，Cisco ASA 多组网关）</span>
          <input
            v-model="form.authGroup"
            type="text"
            placeholder="例如 thirdparty-otp"
            spellcheck="false"
          />
          <span class="hint">
            对应 openconnect 的 <code>--authgroup</code>。留空时如果服务器要求选择 Group，
            连接过程中会弹出选择框，选定后自动写入此配置并重连。
          </span>
        </label>

        <label class="check">
          <input v-model="form.tls12Only" type="checkbox" />
          兼容模式：仅使用 TLS 1.2（部分老版本 ASA 不兼容 TLS 1.3 时勾选）
        </label>
        <span class="hint">
          仅影响 openconnect 的 OpenSSL 握手：勾选后强制 TLS 1.2，
          用于服务器直接返回 <code>handshake failure (alert 40)</code> 的场景；默认关闭。
        </span>

        <!-- 密码模式：密码不落盘，连接时输入 -->
        <p v-if="form.authMethod === 'password'" class="hint">
          密码不会保存在本地，也不会写入配置文件。每次连接时弹窗输入，
          通过 stdin 直接传给 openconnect，不出现在进程参数列表中。
        </p>

        <!-- 证书模式 -->
        <template v-else>
          <div class="field">
            <span class="label">客户端证书 (PEM) *</span>
            <div class="field-inline">
              <input v-model="form.certificatePath" type="text" placeholder="/path/to/cert.pem" spellcheck="false" />
              <button class="btn" @click="browseCert">
                <Icon name="folder" :size="13" /> 浏览
              </button>
            </div>
          </div>
          <div class="field">
            <span class="label">私钥 (PEM，可选)</span>
            <div class="field-inline">
              <input v-model="form.keyPath" type="text" placeholder="/path/to/key.pem" spellcheck="false" />
              <button class="btn" @click="browseKey">
                <Icon name="folder" :size="13" /> 浏览
              </button>
            </div>
          </div>
          <label class="field">
            <span class="label">私钥口令（可选，连接时传入 stdin，不落盘）</span>
            <input v-model="password" type="password" placeholder="留空表示私钥无口令" autocomplete="new-password" />
          </label>
        </template>

        <label class="field">
          <span class="label">服务协议</span>
          <select v-model="form.protocol">
            <option value="">自动探测（推荐）</option>
            <option value="anyconnect">Cisco AnyConnect</option>
            <option value="pulse">Pulse Secure</option>
            <option value="globalprotect">Palo Alto GlobalProtect</option>
            <option value="nc">Juniper Network Connect</option>
            <option value="f5">F5 BIG-IP</option>
            <option value="fortinet">Fortinet FortiGate</option>
          </select>
        </label>

        <div class="field">
          <span class="label">openconnect 路径（留空则使用全局设置）</span>
          <div class="field-inline">
            <input v-model="form.openconnectPath" type="text" placeholder="自动探测" spellcheck="false" />
            <button class="btn" @click="browseBinary">
              <Icon name="folder" :size="13" /> 浏览
            </button>
          </div>
        </div>

        <div v-if="form.servercert" class="field">
          <span class="label">已固定的服务器证书（跳过 CA 校验，仅信任该指纹）</span>
          <div class="field-inline">
            <input :value="form.servercert" readonly />
            <button class="btn" @click="form.servercert = ''">清除</button>
          </div>
          <span class="hint">清除后恢复标准 CA 校验；下次遇到不受信任的证书会重新询问。</span>
        </div>

        <label class="field">
          <span class="label">自定义 openconnect 额外参数</span>
          <textarea
            v-model="form.extraArgs"
            rows="2"
            placeholder="例如：--no-dtls --verbose --servercert pin-sha256:xxxx"
            spellcheck="false"
          />
          <span class="hint">
            参数将原样追加到 openconnect 命令行（位于网关地址之前）。支持单双引号包裹带空格的值。
          </span>
        </label>
      </div>

      <div class="modal-foot">
        <button class="btn" @click="emit('close')">取消</button>
        <button class="btn btn-primary" :disabled="busy" @click="submit">
          {{ busy ? "保存中…" : "保存" }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
code {
  font-family: var(--font-mono);
  font-size: 11.5px;
  background: var(--panel-2);
  border: 1px solid var(--border);
  border-radius: 5px;
  padding: 0 4px;
}
</style>