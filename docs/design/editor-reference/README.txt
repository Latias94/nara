nara editor · Design Kit v0.4
2026-11-05

本版重点
新增两张组件图板与八张不同工具工作区：场景、资源导入、动画、性能诊断、
项目与输入设置、粒子、音频、构建。保留已认可的石墨灰/青绿视觉与紧凑密度，
继续执行 v0.3 的 docking、文档、视图、运行会话和任务规则。

从图片开始
- C01 属性编辑与字段状态：images/nara-components-properties-v0.4.png
- C02 集合、导航与反馈：images/nara-components-navigation-feedback-v0.4.png
- T01 场景与 Inspector：images/nara-scene-inspector-v0.4.png
- T02 资源浏览与导入：images/nara-assets-importer-v0.4.png
- T03 动画编辑：images/nara-animation-editor-v0.4.png
- T04 性能与诊断：images/nara-profiler-console-v0.4.png
- T05 项目与输入设置：images/nara-project-input-settings-v0.4.png
- T06 粒子编辑：images/nara-particle-editor-v0.4.png
- T07 音频混音：images/nara-audio-mixer-v0.4.png
- T08 构建配置与任务：images/nara-build-profiles-v0.4.png

完整规范
nara-editor-design-spec-v0.4.md 共38章，保留前版27章基础，新增：
  第28章：常用组件扩充
  第29–36章：八类工具
  第37章：复用矩阵、图片边界与12项新增实现验收
  第38章：全部图片索引与版本说明
第02章默认工作区已改用新版 Scene / Inspector 图片。
基础颜色、尺寸、输入、校验、撤销和 docking 等仍见前面的相应章节。

交付目录
nara-editor-design-spec-v0.4.md   完整 Markdown
nara-design-tokens.json           基础参数与组件/工具行为映射
images/                          10张v0.4新图和6张继承参考PNG
assets/                          已认可的品牌SVG与应用图标PNG
figures/nara-brand-preview.png    已认可的品牌预览
design-process/                  本轮生图提示词、参考说明与最终图片索引

本轮以PNG、Markdown和JSON为主，不重新生成PDF/DOCX；先前v0.3的PDF/DOCX
保留其版本，不混入v0.4包。Markdown图片使用相对路径，请保持目录结构。
单独的nara-ui-reference-images-v0.4.zip仅包含10张本轮新增PNG；完整包包含
10张新图与6张继承参考，另有规范、token、品牌与设计过程说明。

图片的使用顺序
v0.4图用于评审本轮组件扩充与不同工作情境。六张继承图保留基础组件、
Shader、动画控制器、早期时间轴与多视图/多窗口示例。
v0.2图原有的全局大标签与局部伪停靠标签不再作为外层布局依据；所有工具
标签从所属DockGroup起始端排列，中文和英文默认左对齐。

图片与参数
图片呈现视觉、密度、信息层级与示例状态。细小文字、读数、曲线和连线可能
简化；性能轨道、电平、任务进度等不是实测数据，具体类型、单位、数值与
行为以MD/JSON为准。图中没有画出的错误、加载和恢复状态仍按规范实现。
状态图板中的各小区块是独立控件示例，不应当作一个同时发生的操作现场。
新图是目标设计，不表示全部工具或交互已经实现，也不扩大首版功能承诺。

品牌与颜色
SVG只用于品牌符号、应用图标和字标组合，不用于UI面板图。
16/20逻辑像素使用small光学版，24及以上使用主版。mono采用currentColor，
内联可设置根color，外部图片不假定宿主CSS穿透。
JSON的HEX色值为sRGB编码；绘制管线的线性化、混合与输出按规范实现。
