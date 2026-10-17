# TAM by Example

짧고 **실제로 실행되는** 예제 모음이다. 각 예제는 한 가지 개념을 보여 주고, 모든 코드는 컴파일·실행으로 검증된다(`tools/doccheck.py`). 설명이 더 필요하면 [The TAM Book](../book/README.md)이나 [The TAM Reference](../reference.md)로 간다.

예제의 `// exit: N`은 프로그램이 돌려주는 종료 코드, `// out:`은 출력이다. 실행은 `tamc run 파일.tam`.

| 장 | 내용 |
|---|---|
| [1. 기초](01-basics.md) | 출력, 변수, 산술, 조건, 반복, 함수 |
| [2. 데이터](02-data.md) | 배열, 슬라이스, 구조체, 메서드 |
| [3. 소유권과 빌림](03-ownership.md) | 이동, 참조, `&mut`, 자동 해제 |
| [4. 오류 처리](04-errors.md) | `Option`, `Result`, `?`, 패닉 |
| [5. 문자열과 입출력](05-strings-io.md) | `String`, `tam.fmt`, 표준 입력, 파일 |
| [6. 알고리즘](06-algorithms.md) | 정렬, 이진 탐색, 소수, 피보나치 등 |
| [7. 시스템](07-systems.md) | 포인터, `alloc`, `extern`, 감싸는 산술, 부팅 |
| [8. 모듈과 프로젝트](08-projects.md) | `use`, 패키지, 의존성 |
| [9. 상수, 제네릭, 루프](09-generics-and-constants.md) | `const`, `loop`, 블록 주석, 제네릭 |
