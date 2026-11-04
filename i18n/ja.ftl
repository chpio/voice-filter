# The texts of i18n/en.ftl in Japanese. A paragraph on one line each, as the line break would
# become a space (see src/i18n.rs); an empty line is a new paragraph.

## Stages, in the order of the chain
stage-echo = エコーキャンセル
stage-denoise = ノイズ抑制
stage-highpass = ハイパスフィルター
stage-eq = イコライザー
stage-deesser = ディエッサー
stage-agc = 自動ゲイン制御
stage-compressor = コンプレッサー
stage-expander = エキスパンダー
stage-limiter = リミッター

## The groups of stages in the window
group-echo = エコーキャンセル
group-echo-description = ここで選んだスピーカーのエコーをマイクから取り除きます。ヘッドホンなしの通話向けです。
group-denoise = ノイズ抑制
group-denoise-description = 背景ノイズを取り除きます。上限を下げるとノイズが少し残り、より自然に聞こえます。100 dB ですべて取り除きます。
group-tone = 音色
group-tone-description = 低いゴロゴロ音を取り除き、声を明瞭にし（中低域のこもりを減らし、プレゼンスを上げる）、耳障りな歯擦音を抑えます。
group-level = レベル
group-level-description = 声を一定のレベルにそろえ、単語の中の音量差をならし、話の合間のノイズを下げ、ピークを −1 dBFS 未満に保ちます。

## Parameter titles (the daemon sends the same in English)
param-echo-protect = 声の保護
param-denoise-level = 入力レベル
param-denoise-max-gain = 最大入力ゲイン
param-denoise-limit = 上限
param-highpass-freq = 周波数
param-eq-low-mid = 中低域（250 Hz）
param-eq-presence = プレゼンス
param-eq-presence-freq = プレゼンス周波数
param-deesser-freq = 周波数
param-deesser-threshold = しきい値
param-agc-max-gain = 最大ゲイン
param-agc-threshold = 音声しきい値
param-agc-target = 目標レベル
param-compressor-threshold = しきい値
param-compressor-ratio = レシオ
param-expander-range = 減衰量
param-expander-threshold = 音声しきい値
param-expander-hold = ホールド
param-expander-level = 開くレベル
param-limiter-ceiling = 上限
param-limiter-release = リリース
param-listen-volume = 音量

## What the stages do, inside their rows
help-stage-echo =
    下で選んだスピーカーが再生している音をマイクから取り除きます。ヘッドホンを使わないときでも、通話相手に自分の声が返らないようにします。スピーカーの再生中に、遅延と部屋の響きに合わせて調整されます。

    遅延が 5 ms 増えます。
help-stage-denoise =
    ニューラルネットワーク（DeepFilterNet3）が、ファン、キーボード、交通音などの背景ノイズから声を分離し、ノイズを取り除きます。小さいマイクの音は、うまく動作するレベルまで上げてから聞き取ります。

    遅延が 30 ms 増えます。すべての段の中で最も大きく、オフにしても変わりません（理由はいちばん上の「フィルター」を参照）。
help-stage-highpass = その周波数より下の音をすべて取り除きます。ゴロゴロ音、足音、机にぶつかる音、P や B の破裂音などです。声そのものはそれより上にあります。ノイズ抑制がゴロゴロ音を声と間違えないよう、最初に置かれています。
help-stage-eq = 音色を整えます。声をこもらせたり響かせすぎたりする 250 Hz 付近の中低域を下げ、話し声を明瞭にする 3.5 kHz 付近のプレゼンスを上げます。
help-stage-deesser =
    耳障りなサ行やシャ行の音を抑えます。その周波数より上の帯域が、しきい値よりも大きな割合を占めている間、その帯域を最大 12 dB 下げます。声のほかの部分はそのままです。

    遅延が 1 ms 増えます。
help-stage-agc = 小声でも大声でも、マイクに近くても離れていても、声を一定のレベルにそろえます。数秒かけて調整し（上げるのは毎秒最大 6 dB、下げるのはもっと速く）、話の合間や、ハム音のような一定のノイズ、タイピング中はゲインを保って、それらを持ち上げないようにします。
help-stage-compressor = 自動ゲイン制御では追いつかない、単語や文の中の音量差をならします。しきい値を超える大きな音節を下げ、全体をほぼ同じ量だけ上げるので、同じレベルでも声がより安定して厚みのある音になります。
help-stage-expander = 話の合間を下げ、残ったノイズ（自動ゲイン制御が持ち上げたもの）が単語の間で聞こえないようにします。レベルか、ノイズ抑制の確信度から声を検出するとすぐに開き、ホールド時間が過ぎると再び閉じます。
help-stage-limiter =
    笑い声や咳、大きな声で音が割れないよう、ピークを上限より下に保ちます。1 ms 先を見て、ピークを切り落とすのではなく、その前に下げます。

    遅延が 1 ms 増えます。

## What the parameters do, behind the info button
help-param-echo-protect = スピーカーの再生音に重ねて話すと、エコーキャンセルはエコーと一緒に自分の声も下げてしまいます。話している間は声のこの割合をそのまま残し、残ったエコー（主に音楽）はノイズ抑制が取り除きます。黙っている間はエコーキャンセルが完全に働きます。上げると声がより多く残りますが、スピーカーの音も下に混ざって通りやすくなります。0 % にするとエコーキャンセルだけに任せます。
help-param-denoise-limit = ノイズをどこまで下げてよいか。100 dB で完全に取り除きます。20〜30 dB 程度では背景が少し残り、より自然に聞こえ、声の損失も少なくなります。
help-param-denoise-level = ノイズ抑制が声を聞き取るときのレベル。小さすぎる声はノイズと見なしてミュートしてしまうため、まず小さいマイクの音をこのレベルまで上げ、あとでそのゲインを戻します。出力の音量は変わりません。上げると小さい声がより残りますが、合間にノイズも通りやすくなります。
help-param-denoise-max-gain = ノイズ抑制のために、小さいマイクの音をどこまで上げてよいか。0 dB で持ち上げをオフにします。
help-param-highpass-freq = この周波数より下はすべて取り除かれます。上げるとゴロゴロ音がより取れますが、低い声が細くなります。80 Hz ならどんな声でも安全です。
help-param-eq-low-mid = 250 Hz 付近のゲイン。0 dB より下げると声のこもりや響きが減り、下げすぎると細く聞こえます。
help-param-eq-presence = プレゼンス周波数付近のゲイン。0 dB より上げると話し声がより明瞭で近く聞こえ、上げすぎると耳障りになります。
help-param-eq-presence-freq = プレゼンス帯域の中心。下げると温かく、上げると明るく聞こえます。
help-param-deesser-freq = 歯擦音が始まる位置。この周波数より上の帯域を下げます。下げるとより多くの歯擦音を捉えますが、声もより多く下がります。
help-param-deesser-threshold = 音全体に対するその帯域の割合で、これを超えると働きます。0 dB は音全体です。下げるとより頻繁に強く働き、上げると最も耳障りな歯擦音にだけ働きます。
help-param-agc-target = 声をそろえる平均（RMS）のレベル。−18 dBFS は通話で一般的な値で、ピークのための余裕が残ります。コンプレッサーはこれに追従します。
help-param-agc-max-gain = 小さい声を最大どこまで上げるか。上げるとより小さな声や離れたマイクにも届きますが、そのノイズも上がります。
help-param-agc-threshold = 自動ゲイン制御が声を測るために、ノイズ抑制がどれだけ確実に声を聞き取っている必要があるか（その SN 比）。上げるとノイズをより除外しますが、単語の小さな部分も除外するので、声はあまり上がらなくなります。−20 dB ではノイズ抑制をオフにしたときと同じく、レベルだけで判断します。
help-param-compressor-threshold = このレベルを超えると下げます。下げるとより多くをならし、普通の音節も対象になります。上げると最も大きいものだけです。
help-param-compressor-ratio = しきい値を超えたときにどれだけ下げるか。3:1 では入力が 3 dB 増えても出力は 1 dB しか増えません。1:1 では何もせず、上げるほどならしますが、押しつぶされた音になります。
help-param-expander-range = 合間をどれだけ下げるか。大きくすると合間は静かになりますが、声と合間の切り替わりが目立ちます。0 dB でオフになります。
help-param-expander-threshold = 開くために、ノイズ抑制がどれだけ確実に声を聞き取っている必要があるか（その SN 比）。下げるとノイズでも開きやすくなり、上げるとはっきりした声でだけ開きます。ノイズ抑制がオフのときはレベルだけが基準です。
help-param-expander-hold = 最後の声のあと、どれだけ開いたままにするか。長くすると語尾や短い間が残り、短くすると合間が早く静かになります。
help-param-expander-level = 音がこのレベルより大きくなったときにも開きます。ノイズ抑制が確信を持てない小さな声のためです。下げるとノイズでも開きやすくなります。合間は通常 −55〜−65 dBFS にあります。
help-param-limiter-ceiling = ピークが上がってよい最大値。−1 dBFS にすると、通話のためにコーデックが音を圧縮するときに加わる分の余裕が残ります。
help-param-limiter-release = ピークのあと、どれだけ速く元に戻すか。短くすると音は大きく聞こえますが、早く歪みます。

## What the other rows show
help-filter =
    フィルター全体のオン／オフを切り替えます。オフのとき、アプリにはマイクの音がそのまま渡り、各段は再びオンにするときのために自分のスイッチと設定を保ちます。トレイのアイコンからも切り替えられます。

    オフにしても遅延は変わりません。マイクの音は、フィルターがかける時間（約 37 ms、そのうち 30 ms はノイズ抑制。下の「遅延」の行に全体が表示されます）だけ遅らされます。この遅延をなくすと、オフにするときに音の一部が抜け、オンにするときに空白が入り、通話ではクリック音や途切れとして聞こえます。こうしておけば、切り替えはクロスフェードするだけです。
help-listen-volume = 自分の声を聞くときの音量。モニター専用です。アプリにはフィルターがそろえたレベルの声が渡ります。
help-input-level =
    フィルターを通す前のマイクの音量。バーは −60〜0 dBFS です。

    背景ノイズは、直近 2 秒間で最も静かな瞬間、つまり黙っている間にマイクが拾っている音です。−60 dBFS 未満なら静かです。−45 dBFS を超えるとノイズ抑制がうまく働かず、シャーという音が残ります。マイクブースト（alsamixer ではたいてい「Mic Boost」や「Internal Mic Boost」）を上げているとノイズも上がります。ヘッドセットや USB マイクのほうが、たいていずっと少なく拾います。
help-output-level = フィルターから出る声の音量、つまりアプリに渡る音です。バーは −60〜0 dBFS です。話し声は自動ゲイン制御の目標レベル（−18 dBFS）あたり、合間はそれよりずっと下にあるのが理想です。
help-latency = 声がマイクからアプリに届くまでの時間。マイクのバッファ、仮想マイクの前のバッファ、各段に必要な時間（ノイズ抑制は 30 ms）、アプリのバッファの合計です。
help-processing =
    直近 10 秒間に、10 ms 分の音声が各段を通るのにかかった時間。平均、95 % と 99 % がその時間内に収まった値（最大 2 % 切り上げ）、そして最長です。全体、エコーキャンセルとノイズ抑制の個別（最も時間がかかります）、その他の段の合計を表示します。最長の時間は 10 ms を十分に下回る必要があり、超えると途切れが起きます。オフの段はほとんど時間を使いません。

    PipeWire は音声をサイクルごとに渡し、サイクルあたりのサンプル数がその大きさです（48 サンプルで 1 ms）。フィルターは 480 を要求しますが、より低い遅延が必要なアプリはこれを小さくできます。
help-dropouts =
    フィルターの起動からの累計です。

    無音：時間内に用意できなかった音声で、仮想マイクは代わりに無音を流しました（空白）。アプリが録音を始めた直後に少しあるのは、バッファが落ち着くまでの正常な動作です。

    破棄：遅延を下げ直すために捨てた音声です。たとえばフィルターが一時停止したあとに起きます（飛び）。

    エラー：段の処理が失敗した 10 ms 分の音声で、無音に置き換えられました。

## The window
window-title = Voice Filter
main-menu = メインメニュー
more-information = 詳細
menu-reset = フィルターをリセット(_R)…
reset-heading = フィルターをリセットしますか？
reset-body = すべての段をオンにし、設定を既定値に戻します。マイクとスピーカーはそのままです。
reset-cancel = キャンセル(_C)
reset-confirm = リセット(_R)
missing-title = Voice Filter が実行されていません
missing-description = <tt>voice-filter run</tt> で起動してください
missing-no-bus = セッションバスに接続できません：{ $error }
missing-no-proxy = プロキシを作成できません：{ $error }
copy = コピー
copied = コピーしました
group-input = 入力
group-input-description = フィルターはこのマイクから読み取り、アプリは「Voice Filter」から読み取ります。
filter = フィルター
filter-subtitle = オフのとき、アプリにはマイクの音がそのまま渡ります
microphone = マイク
speakers = スピーカー
default-input = 既定の入力
default-output = 既定の出力
unplugged = { $device }（未接続）
listen = モニター
listen-subtitle = 既定の出力で再生します。ヘッドホンを使ってください
listen-volume = 音量
keep-listening = モニターを維持
keep-listening-subtitle = 再起動後もモニターを再開します。マイクは使用中のままになり、フィルターは常に動作します。
echo-status = 状態
echo-not-in-chain = チェーンにありません
echo-not-running = 動作していません：オフになっているか、「Voice Filter」から録音しているアプリがありません
echo-silent = スピーカーは無音です。取り除くエコーはありません
echo-delay-short = 4 ms 未満
echo-delay = 約 { $delay } ms
echo-hears = スピーカーを { $level } dBFS で聞き取り、そのエコーは { $delay } 後に返ってきます
default-settings = 既定の設定
reset = リセット
group-levels = レベル
level-input = マイク
level-output = Voice Filter
input-not-running = 動作していません：「Voice Filter」から録音しているアプリがありません
input-noise = 背景ノイズ { $floor } dBFS
input-noise-loud = 背景ノイズ { $floor } dBFS、非常に大きい：ノイズ抑制でシャーという音が残ります。マイクブーストを上げているなら下げるか、ヘッドセットを使ってください
group-status = 状態
latency = 遅延
latency-value = アプリまで { $total } ms：マイク { $microphone }、バッファ { $bridge }、処理 { $chain }
processing = 処理時間
processing-mean = 平均
processing-p95 = 95 %
processing-p99 = 99 %
processing-longest = 最長
processing-all = すべての段
processing-other = その他の段
processing-summary = 10 ms フレームあたり { $mean } ms、最長 { $max } ms
processing-nothing = 直近 10 秒間に処理されたものはありません
processing-cycle = サイクルあたり { $samples } サンプル
processing-copied = 10 ms フレームあたり（ms）：{ $columns }
dropouts = 起動からの途切れ
dropouts-value = 無音 { $silence } ms、破棄 { $dropped } ms、エラー { $errors } 件

## The icon in the tray
tray-title = Voice Filter
tray-on = オン
tray-off = オフ：マイクの音はフィルターを通りません
tray-filter = フィルター
tray-open = Voice Filter を開く
