# 다음 세션 가이드

> 작성: 2026-07-28 (M1~M6 1차 완료)

## 즉시 검증
```powershell
cd src-tauri; cargo test --lib     # 51/51
npm run check                       # 0 errors
npm run tauri:dev                   # 우측 하단 위젯
npm run tauri:build                 # 인스톨러 재생성
```

## 완료 상태

| 마일스톤 | 상태 |
|---|---|
| M1 데이터 레이어 PoC | ✅ `GET /api/oauth/usage` HTTP 200 |
| M2 스캐폴딩 + 코어 모듈 | ✅ credentials / usage_client / poller |
| M3 위젯 & 트레이 | ✅ 게이지 3개·창버튼·트레이 색상/툴팁 |
| M4 알림 & 설정 | ✅ 임계값 토스트, 색상·알림·자동시작 설정 |
| M5 히스토리 & 차트 | ✅ SQLite 90일 보관, uPlot 24시간 추이 |
| M6 패키징 | ⚠️ 인스톨러 생성 완료, **메모리 KPI 미달성** |

산출물: `src-tauri/target/release/bundle/nsis/Claude Usage Widget_0.1.0_x64-setup.exe` (2.83MB)

## 사용자 판단이 필요한 것

1. **메모리 목표** — release private bytes 177.3MB로 PRD 150MB를 넘는다.
   앱 자체는 7.6MB이고 나머지는 WebView2 6개 프로세스라 앱 코드로 줄일 여지가 없다.
   → 목표를 상향할지, 네이티브 UI로 갈지 결정 필요 (tasks.md M6 참조)
2. **설치/제거 테스트** — 실제 시스템에 설치가 필요해 보류 중
3. **`/usage` 대조** (M1 잔여) — 대화형 세션에서 `/usage` 실행 후 위젯 값과 비교

## 남은 작업

- 24시간 상주 테스트 (크래시·메모리 누수)
- KPI 검증: `/usage` 대조 10회, 알림 적시성, 재부팅 자동 시작
- 알림 실동작 확인 — **설치 후에** 해야 한다. 설치되지 않은 exe 는
  AppUserModelID 가 없어 Windows 토스트가 표시되지 않을 수 있다
- 위젯 위치 저장·복원 (M3 3.1 잔여, 현재는 매 실행 우측 하단 고정)
- 요일별 집계 뷰 (M5 5.2 잔여)
- (선택) GitHub Releases 자동 업데이트

## 알려진 이슈

- **비공식 엔드포인트** — 변경 시 수정 범위는 `usage_client.rs` + `model.rs` 의 `Raw*`
- **429** — 재시작을 반복하며 폴링하면 요청 제한에 걸린다. 스테일로 강등되고 마지막 값이 유지된다 (실제 확인)
- **트레이 아이콘이 오버플로에 숨음** — Windows 11 기본 정책. 사용자가 고정해야 한다
- **TypeScript 5.9 고정** — svelte-check 4.7 이 TS 7 에서 죽는다
- **트랜스크립트 스캔 비용** — 매 폴링마다 `~/.claude/projects` 를 깊이 3까지 훑는다. 세션이 아주 많아지면 캐시 필요
- **PowerShell 인코딩** — .ps1 은 UTF-8 BOM 필수. `Get-Content`/`Set-Content` 에 `-Encoding utf8` 을 반드시 지정

## 개발 중 밟은 지뢰 (재발 방지)

전부 "조용히 안 되는" 부류였다. 상세는 [architecture.md](architecture.md) §3.5.

1. `CloseRequested` 를 모든 창에서 가로채면 설정 창 X 버튼이 먹통이 된다 → 위젯 창에만
2. 워커 스레드에서 `WebviewWindowBuilder::build()` → 창은 떠도 **웹뷰가 백지**. `run_on_main_thread` 안에서
3. 존재 확인과 생성이 다른 스레드에 있으면 **설정 창이 두 개 겹쳐 뜬다**. 하나를 닫아도 뒤에 남아 "안 닫힌다"로 보이고 하나는 메시지 루프가 멈춘다 → 확인+생성을 메인 스레드 안에서 원자적으로
4. 버튼을 상태 분기 안에 두면 조회 실패 중에 설정을 못 연다 → 헤더는 항상 렌더링
5. 401 응답의 본문을 먼저 읽으면 `Auth` 가 아니라 재시도 대상 `Network` 로 오분류된다 → 상태코드를 먼저 판정
