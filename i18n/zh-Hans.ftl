# The texts of i18n/en.ftl in simplified Chinese. A paragraph on one line each, as the line
# break would become a space (see src/i18n.rs); an empty line is a new paragraph.

## Stages, in the order of the chain
stage-echo = 回声消除
stage-denoise = 降噪
stage-highpass = 高通滤波器
stage-eq = 均衡器
stage-deesser = 齿音消除器
stage-agc = 自动增益控制
stage-compressor = 压缩器
stage-expander = 扩展器
stage-limiter = 限幅器

## The groups of stages in the window
group-echo = 回声消除
group-echo-description = 从麦克风中去除这里所选扬声器的回声，用于不戴耳机通话。
group-denoise = 降噪
group-denoise-description = 去除背景噪声。上限会保留一部分噪声：越低听起来越自然，100 dB 则全部去除。
group-tone = 音色
group-tone-description = 去除低频隆隆声，让声音更清晰（中低频更少浑浊，更多临场感），并柔化刺耳的齿音。
group-level = 电平
group-level-description = 把声音带到稳定的电平，平衡词语内部的响度，压低说话间隙中的噪声，并把峰值保持在 −1 dBFS 以下。

## Parameter titles (the daemon sends the same in English)
param-echo-protect = 人声保护
param-denoise-level = 输入电平
param-denoise-max-gain = 最大输入增益
param-denoise-limit = 上限
param-highpass-freq = 频率
param-eq-low-mid = 中低频（250 Hz）
param-eq-presence = 临场感
param-eq-presence-freq = 临场感频率
param-deesser-freq = 频率
param-deesser-threshold = 阈值
param-agc-max-gain = 最大增益
param-agc-threshold = 语音阈值
param-agc-target = 目标电平
param-compressor-threshold = 阈值
param-compressor-ratio = 压缩比
param-expander-range = 衰减量
param-expander-threshold = 语音阈值
param-expander-hold = 保持时间
param-expander-level = 开启电平
param-limiter-ceiling = 上限
param-limiter-release = 释放时间
param-listen-volume = 音量

## What the stages do, inside their rows
help-stage-echo =
    从麦克风中去除下方所选扬声器播放的声音，这样你不戴耳机时，通话的另一方不会听到自己的声音。扬声器播放时，它会适应延迟和房间。

    增加 5 ms 延迟。
help-stage-denoise =
    神经网络（DeepFilterNet3）把人声与背景噪声（如风扇、键盘或交通）分开，并去除噪声。音量小的麦克风会先被提升到它最适合工作的电平。

    增加 30 ms 延迟，是所有环节中最多的，关闭时也一样（原因见最上方的“过滤器”）。
help-stage-highpass = 去除低于其频率的所有声音：隆隆声、脚步声、碰到桌子的声音以及 P 和 B 音的爆破声。人声本身在这个频率之上。它排在最前面，这样降噪就不会把隆隆声当成人声。
help-stage-eq = 塑造音色：降低 250 Hz 附近的中低频（它让声音听起来闷或浑浊），提升 3.5 kHz 附近的临场感（它让语音更清晰）。
help-stage-deesser =
    柔化刺耳的 S 和 SH 音：当其频率以上的频段在声音中所占比例超过阈值时，就把这个频段压低，最多 12 dB。人声的其余部分保持不变。

    增加 1 ms 延迟。
help-stage-agc = 无论你说话轻还是响，离麦克风近还是远，都把声音带到稳定的电平。它在几秒内逐渐调整（每秒最多升高 6 dB，降低更快），并在说话间隙、持续的噪声（如嗡嗡声）以及打字时保持增益，以免把这些声音提升。
help-stage-compressor = 平衡词语和句子内部的响度，这对自动增益控制来说太快了：它把超过阈值的响亮音节压低，再把整体提升差不多的量，让声音在同样的电平下听起来更平稳、更饱满。
help-stage-expander = 压低说话间隙，让剩余的噪声（以及被自动增益控制提升的噪声）在词与词之间听不到。一听到语音就会打开（根据电平或降噪的把握程度），并在保持时间之后再次关闭。
help-stage-limiter =
    把峰值保持在上限以下，这样笑声、咳嗽或大声的词不会削波。它提前 1 ms 查看，在峰值到来之前就压低，而不是把它切掉。

    增加 1 ms 延迟。

## What the parameters do, behind the info button
help-param-echo-protect = 当你对着扬声器播放的内容说话时，回声消除会连同回声一起压低你的声音。你说话时，你的声音会按这个比例保持原样，剩余的回声（主要是音乐）由降噪去除；你不说话时，回声消除完全生效。越高保留的人声越多，但也会让更多扬声器播放的内容混在下面透过；0 % 则完全交给回声消除。
help-param-denoise-limit = 噪声最多可以压低多少。100 dB 会完全去除；大约 20 到 30 dB 会保留一点背景，听起来更自然，人声损失也更少。
help-param-denoise-level = 降噪听到你声音时的电平。它会把太小的声音当成噪声并静音，所以会先把音量小的麦克风提升到这个电平，之后再把增益撤回：输出的响度保持不变。越高越能保留小声说话，但间隙中会透过更多噪声。
help-param-denoise-max-gain = 为降噪最多可以把音量小的麦克风提升多少。0 dB 关闭提升。
help-param-highpass-freq = 低于这个频率的声音都会被去除。越高去除的隆隆声越多，但低沉的声音会变薄；80 Hz 对所有声音都安全。
help-param-eq-low-mid = 250 Hz 附近的增益。低于 0 dB 时声音不那么闷和浑浊，降得太多则显得单薄。
help-param-eq-presence = 临场感频率附近的增益。高于 0 dB 时语音更清晰、更靠近，升得太多则显得刺耳。
help-param-eq-presence-freq = 临场感频段的中心。越低越温暖，越高越明亮。
help-param-deesser-freq = 齿音开始的位置：它压低这个频率以上的频段。越低捕捉到的齿音越多，但也会压低更多人声。
help-param-deesser-threshold = 该频段在整个声音中所占比例的阈值，超过时开始作用，0 dB 表示整个声音。越低作用越频繁、越强，越高只对最刺耳的齿音作用。
help-param-agc-target = 它把声音平均（RMS）带到的电平。−18 dBFS 是通话的常用值，并为峰值留出余量。压缩器会跟随它。
help-param-agc-max-gain = 小声最多提升多少。越高越能照顾到更小声的说话或更远的麦克风，但也会提升它的噪声。
help-param-agc-threshold = 降噪需要多确定自己听到的是语音（它的信噪比），自动增益控制才会测量它。越高排除的噪声越多，但也会排除词语中较小声的部分，所以提升得更少；−20 dB 则只看电平，就像关闭降噪时一样。
help-param-compressor-threshold = 超过这个电平就会压低。越低平衡得越多，普通音节也会被压低；越高只压低最响的。
help-param-compressor-ratio = 超过阈值后压低多少：在 3:1 时，输入多 3 dB，输出只多 1 dB。1:1 不起作用，越高平衡得越多，但听起来会被挤压。
help-param-expander-range = 把间隙压低多少。越多间隙越安静，但语音和间隙之间的切换越明显；0 dB 关闭它。
help-param-expander-threshold = 降噪需要多确定自己听到的是语音（它的信噪比），它才会打开。越低越容易打开，噪声也会让它打开；越高只对清晰的语音打开。关闭降噪时只看电平。
help-param-expander-hold = 最后一段语音之后保持打开多久。越长越能保留词尾和短暂停顿，越短越快让间隙安静下来。
help-param-expander-level = 当声音比这个电平更响时它也会打开，用于降噪拿不准的小声说话。越低越容易打开，噪声也会让它打开；间隙通常在 −55 到 −65 dBFS。
help-param-limiter-ceiling = 峰值最高可以到多少。−1 dBFS 为编解码器在为通话压缩声音时增加的部分留出余量。
help-param-limiter-release = 峰值之后多快放开。越短听起来越响，但更早失真。

## What the other rows show
help-filter =
    开关整个过滤器。关闭时，应用得到的是原样的麦克风，而每个环节都保留自己的开关和设置，等再次开启时使用。托盘中的图标也可以开关它。

    关闭时延迟保持不变：麦克风会被延迟过滤器所需的时间（约 37 ms，其中 30 ms 来自降噪；下方的“延迟”一行显示全部）。如果去掉这个延迟，关闭时会丢掉一段声音，开启时会插入一段空白，在通话中听起来像咔嗒声或卡顿；这样切换时只是交叉淡化。
help-listen-volume = 你听到自己的音量。只用于监听：应用得到的声音是过滤器带到的电平。
help-input-level =
    麦克风在过滤器之前有多响；音量条从 −60 到 0 dBFS。

    背景噪声是它在最近 2 秒内最安静的时刻，也就是你不说话时它收到的声音。低于 −60 dBFS 算安静；高于 −45 dBFS 时降噪会很吃力，并留下嘶嘶声。调高的麦克风增强（在 alsamixer 中通常叫“Mic Boost”或“Internal Mic Boost”）会提升噪声；耳麦或 USB 麦克风通常收到的噪声少得多。
help-output-level = 声音离开过滤器时有多响，也就是应用得到的声音；音量条从 −60 到 0 dBFS。语音应在自动增益控制的目标电平（−18 dBFS）附近，间隙则远低于它。
help-latency = 声音从麦克风到应用需要多长时间：麦克风的缓冲、虚拟麦克风之前的缓冲、各环节所需的时间（降噪 30 ms）以及应用的缓冲。
help-processing =
    最近 10 秒内，10 ms 音频通过各环节所用的时间：平均值、95 % 和 99 % 的音频所用时间的上限（最多向上取整 2 %）以及最长时间。分别显示全部环节、单独的回声消除和降噪（它们用时最多）以及其余环节合计。最长时间必须远低于 10 ms，否则会出现中断；关闭的环节几乎不占时间。

    PipeWire 按周期传递音频，每周期的样本数就是周期的大小（48 个样本为 1 ms）。过滤器请求 480，需要更低延迟的应用可以把它调小。
help-dropouts =
    自过滤器启动以来。

    静音：没有及时准备好的音频，虚拟麦克风改为播放静音，形成一段空白。应用刚开始录音时有一点是正常的，此时缓冲正在稳定。

    丢弃：为了把延迟重新降下来而丢掉的音频，例如过滤器暂停之后，形成一次跳跃。

    错误：某个环节处理失败的 10 ms 音频，已用静音代替。

## The window
window-title = Voice Filter
main-menu = 主菜单
more-information = 更多信息
menu-reset = 重置过滤器(_R)…
reset-heading = 要重置过滤器吗？
reset-body = 开启所有环节，并把它们的设置恢复为默认值。麦克风和扬声器保持不变。
reset-cancel = 取消(_C)
reset-confirm = 重置(_R)
missing-title = Voice Filter 未在运行
missing-description = 用 <tt>voice-filter run</tt> 启动它
missing-no-bus = 无法连接到会话总线：{ $error }
missing-no-proxy = 无法创建代理：{ $error }
copy = 复制
copied = 已复制
group-input = 输入
group-input-description = 过滤器从这个麦克风读取，应用从“Voice Filter”读取。
filter = 过滤器
filter-subtitle = 关闭时，应用得到原样的麦克风
microphone = 麦克风
speakers = 扬声器
default-input = 默认输入
default-output = 默认输出
unplugged = { $device }（已拔出）
listen = 监听
listen-subtitle = 在默认输出上播放，请使用耳机
listen-volume = 音量
keep-listening = 保持监听
keep-listening-subtitle = 重启后再次监听。麦克风会一直被占用，过滤器会一直运行。
echo-status = 状态
echo-not-in-chain = 不在处理链中
echo-not-running = 未运行：已关闭，或没有应用从“Voice Filter”录音
echo-silent = 扬声器没有声音，没有需要去除的回声
echo-delay-short = 不到 4 ms
echo-delay = 约 { $delay } ms
echo-hears = 听到扬声器的电平为 { $level } dBFS，它们的回声在 { $delay } 后返回
default-settings = 默认设置
reset = 重置
group-levels = 电平
level-input = 麦克风
level-output = Voice Filter
input-not-running = 未运行：没有应用从“Voice Filter”录音
input-noise = 背景噪声 { $floor } dBFS
input-noise-loud = 背景噪声 { $floor } dBFS，非常大：降噪会留下嘶嘶声。如果麦克风增强调高了，把它调低，或者使用耳麦
group-status = 状态
latency = 延迟
latency-value = 到应用 { $total } ms：麦克风 { $microphone }，缓冲 { $bridge }，处理 { $chain }
processing = 处理时间
processing-mean = 平均
processing-p95 = 95 %
processing-p99 = 99 %
processing-longest = 最长
processing-all = 全部环节
processing-other = 其余环节
processing-summary = 每 10 ms 帧 { $mean } ms，最长 { $max } ms
processing-nothing = 最近 10 秒内没有处理任何音频
processing-cycle = 每周期 { $samples } 个样本
processing-copied = 每 10 ms 帧，单位 ms：{ $columns }
dropouts = 启动以来的中断
dropouts-value = 静音 { $silence } ms，丢弃 { $dropped } ms，错误 { $errors } 个

## The icon in the tray
tray-title = Voice Filter
tray-on = 已开启
tray-off = 已关闭：麦克风未经过滤直接通过
tray-filter = 过滤器
tray-open = 打开 Voice Filter
