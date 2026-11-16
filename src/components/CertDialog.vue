<script setup lang="ts">
import { computed } from "vue";
import Icon from "./Icon.vue";
import { dismissCertPrompt, state, trustServerCert } from "../store";

const prompt = computed(() => state.certPrompt);
const profile = computed(() =>
  state.profiles.find((p) => p.id === state.certPrompt?.profileId) ?? null,
);
</script>

<template>
  <div v-if="prompt" class="backdrop" @click.self="dismissCertPrompt">
    <div class="modal" style="max-width: 560px">
      <div class="modal-head">
        <Icon name="shield" :size="16" style="color: var(--warn)" />
        <span>服务器证书未通过校验</span>
      </div>

      <div class="modal-body">
        <div class="banner">
          <Icon name="alert" />
          <div>
            无法用系统信任库验证该服务器证书。这通常意味着对方使用内网 CA 或自签名证书。
            继续连接前请确认该网关确实属于你的组织。
          </div>
        </div>

        <dl class="kv">
          <dt>配置</dt>
          <dd>{{ profile?.name ?? "—" }}</dd>
          <dt>网关</dt>
          <dd>{{ profile?.gateway ?? "—" }}</dd>
          <dt>失败原因</dt>
          <dd style="color: var(--warn)">{{ prompt.reason || "未知" }}</dd>
        </dl>

        <label class="field">
          <span class="label">服务器证书指纹</span>
          <textarea
            :value="prompt.pin"
            readonly
            rows="3"
            spellcheck="false"
            style="word-break: break-all; white-space: pre-wrap"
          />
        </label>

        <p class="hint">
          点「信任并连接」后，该指纹会写入此配置的 servercert 字段，下次连接不再询问。
          这与无条件关闭校验不同：只信任这一张证书，服务器更换证书后会重新询问。
          指纹可用 openssl s_client 或管理员提供的证书信息核对。
        </p>
      </div>

      <div class="modal-foot">
        <button class="btn btn-left" @click="dismissCertPrompt">取消</button>
        <button class="btn btn-primary" @click="trustServerCert()">信任并连接</button>
      </div>
    </div>
  </div>
</template>