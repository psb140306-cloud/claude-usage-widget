# 다음 세션 가이드

> 작성: 2026-07-27 (M1 PoC → /bootstrap → M2 완료)

## 즉시 검증 (다음 세션 첫 1분)
```powershell
cd src-tauri; cargo test --lib     # 33/33 통과해야 함
npm run check                       # 0 errors
npm run tauri:dev                   # 우측 하단에 위젯, 실제 사용량 표시
```
PoC 스크립트로 값 대조: `powershell -ExecutionPolicy Bypass -File scripts\m1-poc-usage.ps1`

## 완료 상태

| 마일스톤 | 상태 |
|---|---|
| M1 데이터 레이어 PoC | ✅ Go — `GET /api/oauth/usage` HTTP 200 |
| M2 2.1 스캐폴딩 | ✅ Tauri 2 + Svelte 5 + SQLite |
| M2 2.2 코어 모듈 | ✅ credentials / usage_client / poller, 실데이터 흐름 확인 |
| M3 3.1 위젯 창 | ✅ 게이지 3개 + 게이지별 리셋 안내 + 계정·플랜·모델·effort·thinking |
| M4 4.2 설정(1차) | ✅ 색상 7종 + 불투명도 + 폴링 주기, 즉시 저장·적용 |

**위젯 표시 항목** (240×215, 작업 영역 우측 하단)
```
성훈  [Max 20x]                    ⚙
세션 (5시간)                      9%
▬▬▭▭▭▭▭▭▭▭   2시간 11분 후 리셋
주간 (7일)                       27%
▬▬▬▬▭▭▭▭▭▭   2일 9시간 후 리셋
주간 (Fable) ●                   29%
▬▬▬▬▭▭▭▭▭▭   2일 9시간 후 리셋
Opus 5 · max · thinking
```

## 다음 작업 우선순위

1. **M3 3.2 트레이** — 사용률 구간별 아이콘 색상, 툴팁 요약(세션 % · 주간 % · 리셋). poller 가 이미 상태를 들고 있으니 구독만 붙이면 된다
2. **M4 4.1 알림** — `notifier.rs` 의 `evaluate()` 가 `todo!()`. 임계값 80/95%, 동일 임계값·동일 리셋 주기 내 1회
3. **M4 4.2 설정 2차** — 알림 on/off, 임계값, 테마, 자동 시작
4. **M5 히스토리** — `history.rs` 의 append/query/prune 이 `todo!()`. poller 성공 시 append 연결
5. **M6 패키징** — release 빌드 메모리 검증(private bytes), NSIS 인스톨러

## 알려진 이슈 / 모니터 대상

- **비공식 엔드포인트** — 변경 시 수정 범위는 `usage_client.rs` + `model.rs` 의 `Raw*` 로 한정
- **429 주의** — 앱을 자주 재시작하며 폴링하면 요청 제한에 걸린다. 걸리면 스테일로 강등되고 마지막 값 + "N분 전 기준"이 표시된다 (실제로 확인함)
- **TypeScript 5.9 고정** — svelte-check 4.7 이 TS 7 에서 죽는다. svelte-check 가 TS 7 을 지원하면 올릴 것
- **트랜스크립트 스캔 비용** — `environment.rs` 가 매 폴링마다 `~/.claude/projects` 를 깊이 3까지 훑는다. 프로젝트·세션이 아주 많아지면 캐시가 필요할 수 있다
- **메모리** — debug 기준. PRD 150MB 검증은 M6 에서 release + private bytes 로
- **PowerShell 인코딩** — .ps1 은 UTF-8 BOM 필수. `Get-Content`/`Set-Content` 로 한글 파일을 다룰 때 `-Encoding utf8` 을 반드시 지정할 것 (지정 안 해서 architecture.md 를 깨뜨린 적 있음)

## 창 수명주기에서 실제로 밟은 지뢰 (재발 방지)

1. `CloseRequested` 를 모든 창에서 가로채면 설정 창 X 버튼이 먹통이 된다 → 위젯 창에만
2. 워커 스레드에서 `WebviewWindowBuilder::build()` 를 부르면 창은 떠도 **웹뷰가 백지**가 된다 → `run_on_main_thread` 안에서
3. 버튼을 상태 분기 안에 두면 조회 실패 중에 설정을 못 연다 → 헤더는 항상 렌더링

상세는 [architecture.md](architecture.md) §3.5.
