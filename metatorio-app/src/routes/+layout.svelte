<script lang="ts">
  // 全局设计系统：随布局第一时间加载，避免首帧变量未定义。
  import "$lib/app.css";
</script>

<slot />
