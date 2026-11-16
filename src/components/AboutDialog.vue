<script setup lang="ts">
import Icon from "./Icon.vue";
import { state } from "../store";

const emit = defineEmits<{ (e: "close"): void }>();

const info = state.info;

const gplSummary = `本程序（Octauri）是自由软件，依据 GNU 通用公共许可证第 3 版
（GNU GPL v3）或您选择的任一更新版本发布。

您可以：
  · 为任何目的运行本程序（含商业用途）；
  · 研究并修改本程序的源代码；
  · 再分发本程序的原版或修改版。

但须遵守以下条件：
  · 再分发时必须附随完整的许可证文本与版权声明；
  · 修改后的版本必须以同一许可证（GPL-3.0-or-later）发布并开放源代码，
    不得闭源分发；
  · 若以二进制形式分发，须同时提供获取对应源代码的途径；
  · 不得对接收者施加 GPL 之外的额外限制。

要点：GPL 允许商业使用，但要求衍生作品同样开源。
本程序不提供任何担保，包括适销性与特定用途适用性。
完整许可证文本：https://www.gnu.org/licenses/gpl-3.0.html

商标说明：Octauri 的名称与图标不属于 GPL 授权范围，项目保留全部权利。
再分发时不得使用本项目名称或图标暗示官方背书。`;
</script>

<template>
  <div class="backdrop" @click.self="emit('close')">
    <div class="modal">
      <div class="modal-head">
        <Icon name="info" :size="16" />
        <span>关于 Octauri</span>
      </div>

      <div class="modal-body">
        <dl class="kv">
          <dt>版本</dt>
          <dd>{{ info?.version ?? "1.0.0" }}</dd>
          <dt>平台</dt>
          <dd>{{ info?.platform ?? "-" }} / {{ info?.arch ?? "-" }}</dd>
          <dt>本程序许可</dt>
          <dd>{{ info?.license ?? "GPL-3.0-or-later" }}</dd>
        </dl>

        <div class="field">
          <span class="label">OpenConnect 版权声明（LGPL-2.1）</span>
          <div class="license-box">
            <p style="margin: 0 0 8px">{{ info?.openconnectLicense ?? "" }}</p>
            <p style="margin: 0 0 8px; white-space: pre-wrap">
              OpenConnect: Copyright © 2009-2025 OpenConnect Contributors.<br />
              OpenConnect 是 OpenConnect 项目的独立作品，本项目仅以子进程方式调用其命令行，
              未对其代码做任何修改，亦未与之静态链接。
            </p>
            <p style="margin: 0">
              许可证原文：https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html<br />
              源码获取：https://gitlab.com/openconnect/openconnect/-/releases<br />
              项目主页：https://www.infradead.org/openconnect/
            </p>
          </div>
        </div>

        <div class="field">
          <span class="label">本程序许可摘要（GPL-3.0-or-later）</span>
          <div class="license-box pre-wrap">{{ gplSummary }}</div>
        </div>

        <div class="field">
          <span class="label">实现说明</span>
          <p class="pre-wrap" style="margin: 0">{{ info?.copyright ?? "" }}</p>
        </div>
      </div>

      <div class="modal-foot">
        <button class="btn btn-primary" @click="emit('close')">关闭</button>
      </div>
    </div>
  </div>
</template>