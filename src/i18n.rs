//! Korean/English UI strings. Keys are the Korean source; returns the source when no English entry exists.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    Ko,
    En,
}

impl Lang {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Ko => "한국어",
            Self::En => "English",
        }
    }
}

/// Look up a UI string. `key` is the Korean source text.
pub fn t(lang: Lang, key: &'static str) -> &'static str {
    match lang {
        Lang::Ko => key,
        Lang::En => en(key).unwrap_or(key),
    }
}

/// English translation table. Keys are Korean source, values are English translations.
const EN_TABLE: &[(&str, &str)] = &[
    ("(없음)", "(none)"),
    ("(출력 없음)", "(no output)"),
    (
        "[오류] 요청 시간이 초과되었습니다.",
        "[error] request timed out.",
    ),
    ("＋ 노드 추가", "+ Add node"),
    ("＋ 컨텍스트", "+ Context"),
    ("▶ ON 전체 실행", "▶ Run all ON"),
    ("▶ 실행", "▶ Run"),
    ("● 키 OK", "● Key OK"),
    ("○ 키 없음", "○ No key"),
    ("→ 연결", "→ Link"),
    ("· 끊김", "· off"),
    ("· 세션 키: 없음", "· session key: none"),
    ("· 세션 키: 입력됨", "· session key: set"),
    ("· 연결됨", "· connected"),
    ("API 키", "API key"),
    ("API 설정 저장", "Save API settings"),
    ("API 설정 초기화", "Reset API settings"),
    (
        "API 설정을 기본값으로 되돌렸습니다.",
        "API settings reset to defaults.",
    ),
    ("✨ 블루프린트", "✨ Blueprint"),
    (
        "프롬프트로 노드 여러 개 만들기",
        "Create multiple nodes from one prompt",
    ),
    (
        "블루프린트 — 프롬프트로 노드 만들기",
        "Blueprint — create nodes from a prompt",
    ),
    (
        "만들 노드 구성을 설명하세요. 예: 기사 요약 → 영문 번역 → 검수 3단계",
        "Describe the nodes to create. E.g.: summarize article → translate to English → review, 3 steps",
    ),
    (
        "최대 12개 노드까지 만들 수 있습니다.",
        "Up to 12 nodes per blueprint.",
    ),
    ("노드 만들기", "Create nodes"),
    ("블루프린트 생성 중…", "Creating blueprint…"),
    ("이미 생성 중입니다.", "Already creating."),
    (
        "Mock에서는 블루프린트를 만들 수 없습니다. Meta API를 선택하세요.",
        "Blueprint needs the Meta API. Mock cannot plan.",
    ),
    ("블루프린트 설명을 입력하세요.", "Enter a blueprint description."),
    ("{n}개 노드를 만들었습니다.", "Created {n} nodes."),
    ("블루프린트 해석 실패: {e}", "Blueprint parse failed: {e}"),
    ("블루프린트 실패: {e}", "Blueprint failed: {e}"),
    (
        "블루프린트 시간 초과. 다시 시도하세요.",
        "Blueprint timed out. Try again.",
    ),
    ("블루프린트 작업이 중단됨", "Blueprint job interrupted"),
    ("이 PC에 키 저장", "Remember key on this PC"),
    ("초기화 확인", "Confirm reset"),
    (
        "한 번 더 누르면 API 설정이 초기화됩니다.",
        "Press again to reset API settings.",
    ),
    ("· 저장된 키: 없음", "· saved key: none"),
    ("· 저장된 키: 있음", "· saved key: set"),
    ("MCP 서버 (stdio)", "MCP servers (stdio)"),
    ("MCP 연결", "MCP"),
    (
        "Mock은 네트워크 없이 데모 출력을 만듭니다. 실제 호출은 Meta Muse API를 선택하세요.",
        "Mock generates demo output offline. Select Meta Muse API for real calls.",
    ),
    ("OFF — 전체 실행 제외", "OFF — excluded from batch run"),
    (
        "같은 이름의 서버가 이미 있습니다.",
        "A server with this name already exists.",
    ),
    ("개별 출력:", "Output:"),
    ("결과 패널 닫기", "Hide results"),
    (
        "결과는 노드의 컨텍스트로 첨부되어 다음 생성에 함께 전송됩니다.",
        "Results are attached as node context and sent with the next generation.",
    ),
    (
        "공백으로 구분, 예: -y @modelcontextprotocol/server-x",
        "Space-separated, e.g. -y @modelcontextprotocol/server-x",
    ),
    ("기능", "Tools"),
    ("닫기", "Close"),
    ("도구 목록 새로고침", "Refresh tools"),
    ("도구 실행", "Run tool"),
    ("도구", "Tool"),
    ("명령", "Command"),
    ("모델:", "Model:"),
    ("모든 출력을 지웠습니다.", "Cleared all outputs."),
    ("바둑판 정렬", "Tile"),
    ("복제", "Clone"),
    (
        "빈 곳 드래그: 캔버스 이동 · 설정에서 AI·MCP 연결",
        "Drag background: pan canvas · AI/MCP in Settings",
    ),
    ("빈 곳 드래그: 캔버스 이동", "Drag background: pan canvas"),
    ("삭제", "Delete"),
    ("상태:", "Status:"),
    (
        "새 노드를 추가했습니다. 타이틀바로 이동하세요.",
        "Node added. Drag it by the title bar.",
    ),
    ("생성 중…", "Generating…"),
    (
        "서버 명령어·인자만 저장됩니다. 도구 결과는 노드의 컨텍스트로 첨부됩니다.",
        "Only server commands/args are stored. Tool results attach as context.",
    ),
    ("서버 추가:", "Add server:"),
    ("서버", "Server"),
    (
        "서버를 추가했습니다. 연결을 눌러보세요.",
        "Server added. Press Connect.",
    ),
    ("설정", "Settings"),
    (
        "세션 키 입력 — 메모리에만 보관",
        "Session key — memory only",
    ),
    ("실행 모드:", "Run mode:"),
    (
        "실행되면 여기에 실시간으로 모입니다",
        "Outputs gather here live",
    ),
    (
        "실행을 누르면 여기 출력이 표시됩니다.",
        "Press Run to show output here.",
    ),
    ("언어 모델", "Language model"),
    ("여기에 프롬프트를 입력하세요…", "Type a prompt here…"),
    ("연결 모드를 취소했습니다.", "Link mode cancelled."),
    (
        "연결 성공: 모델이 응답했습니다.",
        "Connected: the model responded.",
    ),
    ("연결 실패: 시간 초과", "Connection failed: timed out"),
    ("연결 실패: 작업이 중단됨", "Connection failed: job aborted"),
    ("연결 취소", "Cancel link"),
    ("연결 테스트 중…", "Testing…"),
    ("연결 테스트", "Test connection"),
    ("연결", "Connect"),
    (
        "연결된 MCP 서버가 없습니다. 설정 → MCP 연결에서 서버를 연결하세요.",
        "No MCP server connected. Connect one under Settings → MCP.",
    ),
    (
        "연결된 서버의 도구가 여기에 표시됩니다.",
        "Connected tools appear here.",
    ),
    ("연결을 끊었습니다.", "Unlinked."),
    (
        "연결할 노드의 타이틀바를 클릭하세요 (다시 누르면 취소).",
        "Click a node's title bar to link (click again to cancel).",
    ),
    (
        "우선순위: 세션 키 > 저장된 키 > MODEL_API_KEY 환경 변수.",
        "Priority: session key > saved key > MODEL_API_KEY env var.",
    ),
    (
        "저장된 키는 설정 파일에 평문으로 보관됩니다.",
        "Saved keys are stored in plain text in the settings file.",
    ),
    (
        "이 서버의 도구 목록이 없습니다. 설정에서 새로고침하세요.",
        "No tools for this server. Refresh in Settings.",
    ),
    ("이름", "Name"),
    ("이름과 명령어를 입력하세요.", "Enter a name and a command."),
    (
        "이미 실행 중인 작업이 있습니다.",
        "A job is already running.",
    ),
    ("이전 작업을 불러왔습니다.", "Restored previous work."),
    ("인자 (JSON 객체):", "Args (JSON object):"),
    ("인자", "Args"),
    ("입력 ←", "Input ←"),
    ("작업을 취소했습니다.", "Job cancelled."),
    ("작업 중…", "Working…"),
    ("전체 결과 보기", "Show results"),
    ("전체 결과", "All results"),
    ("전체 결과를 복사했습니다.", "Copied all results."),
    ("전체 복사", "Copy all"),
    (
        "전체 실행 완료. 전체 결과 버튼으로 모아보세요.",
        "Batch complete. See All results.",
    ),
    ("제공자", "Provider"),
    ("제목", "Title"),
    ("초", "s"),
    ("추가", "Add"),
    ("출력 복사", "Copy"),
    ("출력 지우기", "Clear outputs"),
    ("출력을 클립보드에 복사했습니다.", "Copied to clipboard."),
    ("취소", "Cancel"),
    ("새창으로 열기", "Open in new window"),
    ("전체 결과 — 전문", "Full results — full text"),
    ("라이브 결과", "Live results"),
    ("라이브 결과창 열기", "Open live results"),
    ("보기:", "View:"),
    ("필터:", "Filter:"),
    ("노드별", "Per node"),
    ("합침", "Combined"),
    ("전체", "All"),
    ("실행 중", "Running"),
    ("오류만", "Errors only"),
    ("자동 스크롤", "Auto-scroll"),
    ("자동 줄바꿈", "Word wrap"),
    ("파이썬", "Python"),
    ("파이썬 코드:", "Python code:"),
    ("파이썬 코드를 입력하세요…", "Enter Python code…"),
    (
        "노드 '{title}' 파이썬 실행 중…",
        "Node '{title}' running Python…",
    ),
    ("결과 알람", "Result alarms"),
    (
        "전체·선택 실행이 끝나면 켜진 채널로 결과 요약을 보냅니다.",
        "Sends a result summary to enabled channels when a batch finishes.",
    ),
    ("텔레그램 봇", "Telegram bot"),
    ("봇 토큰", "Bot token"),
    ("챗 ID", "Chat ID"),
    ("디스코드 웹훅", "Discord webhook"),
    ("웹훅 주소", "Webhook URL"),
    ("왓츠앱 Cloud API", "WhatsApp Cloud API"),
    ("토큰", "Token"),
    ("발신 번호 ID", "Sender number ID"),
    ("수신 번호", "Recipient number"),
    (
        "알람 키·주소는 설정 파일에 평문으로 보관됩니다.",
        "Alarm keys and URLs are stored in plain text in the config file.",
    ),
    ("테스트 알람", "Test alarm"),
    ("알람 전송 중…", "Sending alarm…"),
    ("알람 전송됨: {sent}", "Alarm sent: {sent}"),
    ("알람 실패: {e}", "Alarm failed: {e}"),
    ("알람 실패: 시간 초과", "Alarm failed: timed out"),
    ("알람 실패: 작업이 중단됨", "Alarm failed: job aborted"),
    (
        "켜진 알람 채널이 없습니다. 채널을 켜고 값을 입력하세요.",
        "No alarm channel is enabled. Enable one and fill in the values.",
    ),
    (
        "Symphony Canvas 테스트 알람입니다.",
        "This is a Symphony Canvas test alarm.",
    ),
    ("이미지 표시 크기", "Image display size"),
    ("출력 이미지 높이", "Output image height"),
    (
        "결과 패널·라이브 창·출력 창의 이미지에 함께 적용됩니다.",
        "Applies to images in the results panel, live window, and output window.",
    ),
    ("글자 크기", "Font size"),
    ("고정폭", "Monospace"),
    ("복사", "Copy"),
    ("파일", "File"),
    ("새로 시작", "New"),
    ("새로 시작할까요?", "Start new?"),
    (
        "저장하지 않은 내용이 사라집니다.",
        "Unsaved changes will be lost.",
    ),
    ("저장하고 새로 만들기", "Save and start new"),
    ("저장하지 않고 새로 만들기", "Start new without saving"),
    ("새로 시작했습니다.", "Started a new canvas."),
    ("단축키: Ctrl+S", "Shortcut: Ctrl+S"),
    ("타이틀바 드래그: 노드 이동", "Title-bar drag: move node"),
    ("프롬프트 입력:", "Prompt:"),
    ("해제", "Disconnect"),
    ("환경변수 키: 미설정", "Env key: missing"),
    ("환경변수 키: 설정됨", "Env key: set"),
    (
        "노드 입력을 연결하면 출력이 다음 노드로 이어집니다",
        "Link a node input to chain outputs into the next node",
    ),
    ("[오류] {e}", "[error] {e}"),
    ("'{}' 이동 중…", "Moving '{}'…"),
    ("{}자", "{} chars"),
    ("{}회", "×{}"),
    (
        "{mode_label} {started}개 실행 중 (OFF {skipped_off}개·빈 프롬프트 {skipped_empty}개 제외)",
        "{mode_label}: {started} running (OFF {skipped_off} · empty {skipped_empty} skipped)",
    ),
    ("MCP '{name}' 연결 실패: {e}", "MCP '{name}' failed: {e}"),
    ("MCP '{name}' 연결됨.", "MCP '{name}' connected."),
    (
        "MCP '{server}'에 연결되어 있지 않습니다.",
        "MCP '{server}' is not connected.",
    ),
    ("노드 '{}' 삭제됨.", "Deleted '{}'."),
    ("노드 '{}' 실행 실패.", "'{}' failed."),
    ("노드 '{}' 실행 완료.", "'{}' done."),
    (
        "노드 '{}': 프롬프트가 비어 있습니다.",
        "'{}': prompt is empty.",
    ),
    (
        "노드 '{title}' 생성 중…({client_name})",
        "Generating '{title}'…({client_name})",
    ),
    (
        "도구 '{tool}' 결과를 컨텍스트에 추가했습니다.",
        "Added '{tool}' result to context.",
    ),
    ("도구 '{tool}' 호출 실패: {e}", "Tool '{tool}' failed: {e}"),
    (
        "도구 목록 실패({name}): {e}",
        "Tool list failed ({name}): {e}",
    ),
    ("도구 실행 — {title}", "Run tool — {title}"),
    (
        "도구 인자는 JSON 객체여야 합니다. 예: {}",
        "Tool args must be a JSON object, e.g. {}",
    ),
    ("도구 인자는", "Tool args"),
    ("도구 호출 시간 초과.", "Tool call timed out."),
    ("도구 호출 중…", "Calling tool…"),
    ("실행 모드: {}", "Run mode: {}"),
    ("연결 실패: {e}", "Connection failed: {e}"),
    ("자동 저장 실패: {e}", "Autosave failed: {e}"),
    ("작업 중단됨", "job aborted"),
    (
        "전체 {total} · ON {on_count}",
        "Total {total} · ON {on_count}",
    ),
    ("프롬프트 {}", "Prompt {}"),
    ("{} · 폰트: {name}", "{} · font: {name}"),
    ("Mock (데모)", "Mock (demo)"),
    ("다중(병렬)", "Parallel"),
    ("순차", "Sequential"),
    ("연결 순서(A→B→C)", "Chain (A→B→C)"),
    ("켜진 노드를 전부 동시에 실행", "Run all ON nodes at once"),
    (
        "캔버스 순서대로 하나씩 실행",
        "Run one at a time, canvas order",
    ),
    (
        "입력 연결을 따라 하나씩 실행, 앞 출력이 뒤 입력으로 전달",
        "Run one at a time along input links, feeding outputs forward",
    ),
    ("언어", "Language"),
    ("테마", "Theme"),
    ("줌:", "Zoom:"),
    (
        "휠: 확대/축소 · 긴 글 위는 내용 스크롤",
        "Wheel: zoom · long text scrolls instead",
    ),
    (
        "Shift+드래그: 박스 선택 · 선 ×·카드 ×: 연결 끊기 · Delete: 선택 삭제",
        "Shift+drag: box select · × on link/card: unlink · Delete: remove selection",
    ),
    (
        "클릭: 연결 끊기 · 우클릭: 메뉴",
        "Click: unlink · right-click: menu",
    ),
    ("클릭: 노드로 이동", "Click: go to node"),
    ("연결 끊기", "Unlink"),
    ("양방향으로 만들기", "Make bidirectional"),
    ("단방향으로 되돌리기", "Back to one-way"),
    ("단방향으로 되돌렸습니다.", "Reverted to one-way link."),
    ("원본 실행: {t}", "Run source: {t}"),
    ("대상 실행: {t}", "Run target: {t}"),
    ("{n}개 선택됨", "{n} selected"),
    ("모두 켜기", "Enable all"),
    ("모두 끄기", "Disable all"),
    ("모두 삭제", "Delete all"),
    ("{n}개 노드 삭제됨.", "{n} nodes deleted."),
    (
        "선택 {n}개를 지우려면 Delete를 한 번 더 누르세요.",
        "Press Delete again to remove {n} selected nodes.",
    ),
    ("맞춤", "Reset"),
    ("공유:", "Share:"),
    ("메일", "Email"),
    ("메일 앱을 열었습니다.", "Opened the mail app."),
    ("메일 앱을 열지 못했습니다.", "Could not open the mail app."),
    (
        "캡션을 복사했습니다. 페이스북에 붙여넣으세요.",
        "Caption copied. Paste it on Facebook.",
    ),
    ("Threads 작성 창을 열었습니다.", "Opened Threads composer."),
    (
        "캡션을 복사했습니다. 인스타그램에 붙여넣으세요.",
        "Caption copied. Paste it on Instagram.",
    ),
    ("문서 저장", "Save doc"),
    ("문서를 저장했습니다.", "Document saved."),
    ("일반", "Normal"),
    ("에이전트", "Agent"),
    ("모드:", "Mode:"),
    ("단계", " steps"),
    ("모양", "Style"),
    ("파랑", "Blue"),
    ("초록", "Green"),
    ("주황", "Orange"),
    ("회색", "Gray"),
    ("둥글게", "Round"),
    ("약간", "Soft"),
    ("각지게", "Sharp"),
    (
        "에이전트는 연결된 MCP 서버가 필요합니다.",
        "Agent mode needs a connected MCP server.",
    ),
    ("[에이전트 {n}단계]", "[agent: {n} steps]"),
    ("노드별 출력", "Per node"),
    ("세부 옵션", "Details"),
    ("온도:", "Temperature:"),
    ("최대 토큰:", "Max tokens:"),
    ("기본값", "Default"),
    ("노드를 바둑판으로 정렬했습니다.", "Tiled the nodes."),
    ("노드를 복제했습니다.", "Node cloned."),
    ("노드를 연결했습니다.", "Nodes linked."),
    (
        "노드를 추가하고 실행을 눌러보세요.",
        "Add a node and press Run.",
    ),
    ("노드형 멀티 프롬프트", "Node-based multi prompt"),
    ("직접", "Custom"),
    ("테마 미리 보기", "Theme preview"),
    ("실행", "Run"),
    ("노드", "Nodes"),
    ("화면", "Display"),
    ("편집", "Edit"),
    ("저장", "Save"),
    ("배경:", "Background:"),
    ("격자:", "Grid:"),
    ("도트", "Dots"),
    ("선", "Lines"),
    ("없음", "None"),
    ("미니맵 끄기", "Hide minimap"),
    ("미니맵 켜기", "Show minimap"),
    ("우하단 전체 지도 표시 전환", "Toggle overview map"),
    ("◀ 결과", "◀ Results"),
    ("결과 패널 열기 (Ctrl+R)", "Open results (Ctrl+R)"),
    (
        "좌우 드래그: 너비 조절 · 더블클릭: 원래대로",
        "Drag sideways: resize · double-click: reset",
    ),
    ("순환 연결은 만들 수 없습니다.", "Links cannot form a loop."),
    ("단축키: Ctrl+Enter", "Shortcut: Ctrl+Enter"),
    ("단축키: Esc", "Shortcut: Esc"),
    ("단축키: Ctrl+Z", "Shortcut: Ctrl+Z"),
    ("단축키: Ctrl+Y", "Shortcut: Ctrl+Y"),
    ("■ 전체 취소", "■ Cancel all"),
    ("되돌리기", "Undo"),
    ("다시 하기", "Redo"),
    ("되돌렸습니다.", "Undone."),
    ("다시 적용했습니다.", "Redone."),
    ("전체 {n}개 작업을 멈췄습니다.", "Stopped {n} jobs."),
    ("진행 중인 작업이 없습니다.", "No jobs running."),
    ("전체 보기", "Fit all"),
    ("모든 노드가 들어오게 맞춥니다", "Fit all nodes into view"),
    ("줌 100%·원점으로 되돌립니다", "Reset zoom and origin"),
    ("노드를 격자로 가지런히 놓습니다", "Arrange nodes in a grid"),
    ("지우기 확인", "Confirm clear"),
    ("한 번 더 누르면 지워집니다.", "Press again to clear."),
    ("내보내기", "Export"),
    ("가져오기", "Import"),
    ("캔버스를 JSON 파일로 저장합니다", "Save canvas as JSON"),
    ("JSON 파일에서 캔버스를 불러옵니다", "Load canvas from JSON"),
    ("캔버스를 내보냈습니다.", "Canvas exported."),
    ("캔버스를 가져왔습니다.", "Canvas imported."),
    ("가져오기 실패: {e}", "Import failed: {e}"),
    ("저장된 파일 열기", "Open saved file"),
    ("단축키: Ctrl+O", "Shortcut: Ctrl+O"),
    ("저장된 파일을 열었습니다.", "Opened saved file."),
    ("검색:", "Search:"),
    ("제목 검색", "Search titles"),
    ("일치하는 노드 없음", "No matches"),
    ("'{}'로 이동했습니다.", "Jumped to '{}'."),
    ("전체 보기로 맞췄습니다.", "Fitted to view."),
    ("정보", "About"),
    ("버전", "Version"),
    (
        "노드형 멀티 프롬프트 캔버스 (Meta Muse API + MCP)",
        "Node-based multi-prompt canvas (Meta Muse API + MCP)",
    ),
    ("라이선스: MIT", "License: MIT"),
    (
        "API 키는 '이 PC에 키 저장'을 켤 때만 디스크에 저장됩니다.",
        "API keys are saved to disk only when 'Remember key on this PC' is on.",
    ),
    (
        "단축키: Ctrl+Enter 실행 · Ctrl+N 노드 · Ctrl+S 저장 · Ctrl+Z 되돌리기 · Esc 전체 취소",
        "Shortcuts: Ctrl+Enter run · Ctrl+N node · Ctrl+S save · Ctrl+Z undo · Esc cancel all",
    ),
    ("이 노드만 실행합니다", "Run only this node"),
    ("진행 중인 작업을 멈춥니다", "Stop the running job"),
    ("MCP 도구 결과를 붙입니다", "Attach an MCP tool result"),
    ("같은 설정으로 복사합니다", "Clone with the same settings"),
    (
        "출력 포트에서 드래그해도 연결됩니다",
        "You can also drag from the output port",
    ),
    ("두 번 누르면 삭제됩니다", "Press twice to delete"),
    ("이전 출력", "Previous"),
    ("이전 닫기", "Hide previous"),
    ("이전 출력:", "Previous:"),
    ("덮어쓰기 전 출력을 봅니다", "Show output before overwrite"),
    ("{}초 전", "{}s ago"),
    ("{}분 전", "{}m ago"),
    ("{}시간 전", "{}h ago"),
    ("{}일 전", "{}d ago"),
    ("저장했습니다.", "Saved."),
    ("아직 노드가 없습니다", "No nodes yet"),
    (
        "버튼을 눌러 첫 프롬프트를 만드세요",
        "Press the button to create your first prompt",
    ),
    ("＋ 첫 노드 만들기", "+ Create first node"),
    ("이미지 노드를 만들었습니다.", "Created an image node."),
    (
        "터미널에서 같은 명령을 직접 실행해 STDIO 응답을 확인하세요.",
        "Run the same command in a terminal to inspect its STDIO.",
    ),
    (
        "연결할 노드의 동그라미에 놓거나 타이틀바를 클릭하세요.",
        "Drop on another node's circle, or click its title bar.",
    ),
    (
        "입력 동그라미를 클릭하거나 타이틀바를 클릭하세요.",
        "Click an input circle or a title bar to complete the link.",
    ),
    (
        "노드 오른쪽 동그라미를 눌러 드래그하면 연결됩니다",
        "Drag from a node's right circle to link nodes",
    ),
    (
        "연결: 동그라미 드래그 · 카드의 → 연결 버튼 · L(선택 연결)",
        "Link: drag a circle · → Link button on card · L (link selection)",
    ),
    (
        "연결할 노드를 2개 이상 선택하세요.",
        "Select 2 or more nodes to link.",
    ),
    ("더 연결할 게 없습니다.", "Nothing more to link."),
    ("{n}개 연결했습니다.", "Linked {n}."),
    ("이미 연결되어 있습니다.", "Already linked."),
    ("기본", "Default"),
    ("제목만 표시 전환", "Toggle title-only view"),
    ("모두 접기", "Collapse all"),
    ("모두 펼치기", "Expand all"),
    ("모든 노드를 접었습니다.", "Collapsed all nodes."),
    ("모든 노드를 펼쳤습니다.", "Expanded all nodes."),
    ("저장 중…", "Saving…"),
    ("저장됨", "Saved"),
    ("저장되지 않은 변경 있음", "Unsaved changes"),
    (
        "드래그: 이동 · 더블클릭: 제목 편집",
        "Drag: move · double-click: rename",
    ),
    ("MCP 연결 필요", "Needs MCP connection"),
    ("이미지:", "Image:"),
    ("이미지 선택", "Choose image"),
    ("이미지 지우기", "Remove image"),
    ("대표 이미지를 설정했습니다.", "Set the node image."),
    ("(이미지를 찾을 수 없음)", "(image not found)"),
    ("0이면 기본값 사용", "0 = use default"),
    ("보기", "Preview"),
    ("삭제 확인", "Confirm delete"),
    ("한 번 더 누르면 삭제됩니다.", "Press again to delete."),
    ("대기", "Idle"),
    ("완료", "Done"),
    ("오류", "Error"),
    ("실행 중…", "Running…"),
    ("▶ 재실행", "▶ Re-run"),
    ("[취소됨] {e}", "[cancelled] {e}"),
    (
        "API 키가 없습니다. 설정에서 키를 입력하세요.",
        "No API key. Enter a key in Settings.",
    ),
    ("네트워크 오류: {e}", "Network error: {e}"),
    ("서버 오류 {code}: {body}", "Server error {code}: {body}"),
    ("응답 해석 실패: {e}", "Failed to parse response: {e}"),
    (
        "제한 시간 초과. 다시 실행하세요.",
        "Timed out. Please run again.",
    ),
    ("취소됨.", "Cancelled."),
    ("MCP 오류: {e}", "MCP error: {e}"),
    (
        "[건너뜀] 연결된 이전 노드가 실패했습니다.",
        "[skipped] a linked earlier node failed.",
    ),
    (
        "노드 '{}' 건너뜀 (이전 실패).",
        "Skipped '{}' (earlier failure).",
    ),
    (
        "전체 실행 완료 (실패 {failed}개). 전체 결과 버튼으로 모아보세요.",
        "Batch done ({failed} failed). See Combined results.",
    ),
    (" 다시 실행해 보세요.", " Try again."),
    ("[오류] {e}{hint}", "[error] {e}{hint}"),
    ("기록", "History"),
    ("선택 노드", "Selected node"),
    ("선택 해제", "Deselect"),
    ("ON: 실행함", "ON: will run"),
    ("OFF: 건너뜀", "OFF: skipped"),
    ("너비 {pct}%", "Width {pct}%"),
    ("너비 {w}% · 높이 {h}%", "Width {w}% · Height {h}%"),
    (
        "모서리 드래그: 크기 조절 · 더블클릭: 원래대로",
        "Corner drag: resize · double-click: reset",
    ),
    ("연결 {n}개", "{n} links"),
    ("이미지", "Image"),
    ("오디오", "Audio"),
    ("요약", "Summarize"),
    ("영어로 번역", "Translate to EN"),
    ("한국어로 번역", "Translate to KO"),
    ("문장 다듬기", "Polish"),
    ("불릿 정리", "Bullets"),
    ("표로 정리", "Table"),
    ("아이디어 확장", "Expand ideas"),
    ("반박·비판", "Critique"),
    ("질문 만들기", "Questions"),
    ("코드 설명", "Explain code"),
    ("코드 작성", "Write code"),
    ("이메일 작성", "Email"),
    ("SNS 문구", "SNS copy"),
    ("제목 짓기", "Titles"),
    ("키워드 추출", "Keywords"),
    ("실행 계획", "Plan"),
    ("장단점 분석", "Pros & cons"),
    ("쉽게 설명", "Explain simply"),
    (
        "노드 '{title}' {kind} 생성 중…",
        "Generating {kind} for '{title}'…",
    ),
    ("[이미지 저장됨] {path}", "[image saved] {path}"),
    ("[오디오 저장됨] {path}", "[audio saved] {path}"),
    ("오디오를 재생합니다.", "Playing audio."),
    (
        "오디오를 열 수 없습니다: {path}",
        "Cannot open audio: {path}",
    ),
    ("▶ 재생", "▶ Play"),
    ("예약 실행", "Scheduled run"),
    ("예약 켜기", "Enable schedule"),
    ("날짜:", "Date:"),
    ("시간:", "Time:"),
    ("YYYY-MM-DD", "YYYY-MM-DD"),
    ("HH:MM", "HH:MM"),
    ("날짜를 비우면 오늘로 봅니다", "Empty date means today"),
    ("반복:", "Repeat:"),
    ("한 번만", "Once"),
    ("매일", "Daily"),
    ("간격 반복", "Repeat on interval"),
    ("간격:", "Every:"),
    ("시간", "hr"),
    ("분", "min"),
    ("이 노드 예약 켜기", "Enable schedule for this node"),
    ("대기:", "Wait:"),
    ("루프 횟수 (1=한 번만):", "Loop count (1=once):"),
    ("성공하면 중단", "Stop on success"),
    (
        "에러가 아닌 출력이 나오면 루프를 멈춥니다",
        "Stop looping on non-error output",
    ),
    ("출력에 포함되면 중단:", "Stop when output contains:"),
    ("예약 실행: 노드 '{title}'", "Scheduled run: node '{title}'"),
    ("{n}회 실행", "{n} runs done"),
    ("다음 실행: {at}", "Next run: {at}"),
    ("날짜·시간 형식이 틀렸습니다.", "Bad date/time format."),
    ("예약이 꺼져 있습니다.", "Schedule is off."),
    (
        "예약 시각이 되어 전체 실행을 시작합니다.",
        "Scheduled time reached. Starting batch run.",
    ),
    ("▶ 선택 {n}개 실행", "▶ Run {n} selected"),
    (
        "선택한 노드만 실행합니다 (Shift+클릭·Shift+드래그로 선택)",
        "Run only the selected nodes (Shift+click / Shift+drag to select)",
    ),
    (
        "먼저 노드를 선택하세요 (Shift+클릭·Shift+드래그).",
        "Select nodes first (Shift+click / Shift+drag).",
    ),
    (
        "선택한 노드는 실행할 게 없습니다 (OFF·빈 프롬프트·실행 중 제외).",
        "Nothing runnable in the selection (OFF, empty or busy nodes excluded).",
    ),
    ("선택 실행", "Selected run"),
    (
        "선택 실행 완료. 전체 결과 버튼으로 모아보세요.",
        "Selected run complete. See All results.",
    ),
    (
        "선택 실행 완료 (실패 {failed}개). 전체 결과 버튼으로 모아보세요.",
        "Selected run done ({failed} failed). See Combined results.",
    ),
    ("← 이 노드에 연결", "← Link to this node"),
    ("누르면 바로 연결됩니다", "Click to link immediately"),
    ("편집", "Edit"),
    ("제목을 편집합니다", "Edit the title"),
    ("＋ 추가", "＋ Add"),
    ("＋ 추가 닫기", "＋ Close"),
    ("연결할 노드를 골라 잇습니다", "Pick a node to link"),
    ("전체 끊기", "Unlink all"),
    ("입력 연결을 모두 끊습니다", "Unlink all inputs"),
    ("입력 연결을 모두 끊었습니다.", "Unlinked all inputs."),
    ("× 끊기", "× Unlink"),
    ("'{title}' 연결을 끊습니다.", "Unlink '{title}'."),
    ("더 이을 노드가 없습니다", "No more nodes to link"),
    (
        "어시스턴트 말투·역할 지시 (선택):",
        "Assistant tone/role instruction (optional):",
    ),
    (
        "예: 너는 친절한 한국어 튜터야.",
        "E.g.: You are a friendly Korean tutor.",
    ),
    ("이 컨텍스트를 지웁니다", "Remove this context"),
    (
        "OFF 상태입니다. 켜야 실행됩니다",
        "This node is OFF. Turn it on to run",
    ),
    ("실행 중입니다", "Running"),
    (
        "설정에서 MCP 서버를 먼저 연결하세요",
        "Connect an MCP server in Settings first",
    ),
    ("크게 보기", "Zoom"),
    (
        "출력을 큰 창으로 봅니다",
        "View the output in a large window",
    ),
    ("맵 크기:", "Map size:"),
    ("양방향으로 연결했습니다.", "Linked both ways."),
];

fn en(key: &str) -> Option<&'static str> {
    EN_TABLE.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_keys_translate() {
        assert_eq!(t(Lang::En, "복제"), "Clone");
        assert_eq!(t(Lang::En, "실행 모드:"), "Run mode:");
        assert_eq!(t(Lang::Ko, "복제"), "복제");
    }

    #[test]
    fn missing_key_falls_back_to_korean() {
        assert_eq!(t(Lang::En, "없는 키"), "없는 키");
    }

    #[test]
    fn templates_keep_placeholders() {
        let s = t(Lang::En, "노드 '{}' 삭제됨.").replace("{}", "A");
        assert_eq!(s, "Deleted 'A'.");
    }

    /// Check that every key used via t() across all sources has an English entry.
    /// Without an English entry the Korean shows through and hides the gap, so
    /// this test fails if even one key is missing.
    #[test]
    fn every_ui_key_has_english() {
        const SOURCES: &[&str] = &[
            include_str!("app.rs"),
            include_str!("cards.rs"),
            include_str!("panels.rs"),
            include_str!("state.rs"),
            include_str!("run.rs"),
            include_str!("node.rs"),
            include_str!("store.rs"),
            include_str!("theme.rs"),
            include_str!("llm.rs"),
            include_str!("mcp.rs"),
            include_str!("agent.rs"),
            include_str!("share.rs"),
            include_str!("fonts.rs"),
            include_str!("usability.rs"),
            include_str!("main.rs"),
            include_str!("i18n.rs"),
        ];
        // Keys intentionally left untranslated: used only by fallback-behavior tests.
        const ALLOW_UNTRANSLATED: &[&str] = &["없는 키"];
        let mut missing: Vec<String> = Vec::new();
        for src in SOURCES {
            for key in extract_t_keys(src) {
                if !EN_TABLE.iter().any(|(k, _)| *k == key)
                    && !ALLOW_UNTRANSLATED.contains(&key)
                    && !missing.iter().any(|m| m == key)
                {
                    missing.push(key.to_string());
                }
            }
        }
        assert!(missing.is_empty(), "영어 번역이 없는 키: {missing:?}");
    }

    /// Extract only the keys from t(first-arg, "key") calls in source.
    /// Skip t() inside strings/comments and lookalike names such as shorten(.
    fn extract_t_keys(src: &str) -> Vec<&str> {
        let b = src.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i + 1 < b.len() {
            // Skip string literals wholesale (including escapes).
            if b[i] == b'"' {
                let mut j = i + 1;
                while j < b.len() {
                    if b[j] == b'\\' {
                        j += 2;
                    } else if b[j] == b'"' {
                        break;
                    } else {
                        j += 1;
                    }
                }
                i = j + 1;
                continue;
            }
            // Skip single-line comments.
            if b[i] == b'/' && b[i + 1] == b'/' {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            if b[i] == b't'
                && b[i + 1] == b'('
                && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_'))
            {
                // Skip the first argument and read the second-argument string.
                let mut j = i + 2;
                let mut depth = 0usize;
                let mut in_str = false;
                let mut key_start: Option<usize> = None;
                let mut key_end = 0usize;
                while j < b.len() {
                    let c = b[j];
                    if in_str {
                        if c == b'\\' {
                            j += 2;
                            continue;
                        }
                        if c == b'"' {
                            in_str = false;
                            if key_start.is_some() {
                                key_end = j;
                                break;
                            }
                        }
                        j += 1;
                        continue;
                    }
                    match c {
                        b'"' => {
                            in_str = true;
                            if depth == 0 && key_start.is_none() {
                                // Past the comma after the first argument, treat the first string as the key.
                                key_start = Some(j + 1);
                            }
                        }
                        b'(' => depth += 1,
                        b')' => {
                            if depth == 0 {
                                break;
                            }
                            depth -= 1;
                        }
                        _ => {}
                    }
                    j += 1;
                }
                // If the first argument contained a string (rare case), it is not a key.
                // Since t()'s first argument is an identifier, check that a comma
                // appeared before the string.
                if let Some(s) = key_start {
                    let before = &src[i..s];
                    if before.contains(',') {
                        out.push(&src[s..key_end]);
                    }
                }
                i = j + 1;
            } else {
                i += 1;
            }
        }
        out
    }

    #[test]
    fn extractor_finds_keys() {
        let src = "let a = t(lang, \"복제\");\nlet b = t(\n Lang::En, \"삭제\");\n// t(lang, \"주석\")\nshorten(\"x\");";
        // Skip keys inside comments.
        assert_eq!(extract_t_keys(src), vec!["복제", "삭제"]);
    }
}
