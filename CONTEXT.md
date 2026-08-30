# toc

toc은 원본을 바꾸지 않고 text와 bytes의 변환 결과를 살펴보는 로컬 workbench다.

## Language

**Input**:
사용자가 제공하고 변환 중에도 보존되는 원본 text 또는 bytes다.
_Avoid_: Source buffer

**Transform**:
명시된 입력·출력 규칙에 따라 bytes를 다른 bytes로 바꾸는 이름 있는 연산이다.
_Avoid_: Operation

**Pipeline**:
Input에 순서대로 적용되는 Transform의 목록이다.
_Avoid_: Chain

**Output**:
최종 Pipeline 결과 또는 선택한 Transform 단계의 결과다. Input과 독립적으로 살펴본다.
_Avoid_: Result pane

**View**:
Output을 Smart, Text, Hex 또는 Trace 형태로 살펴보는 표현 방식이다.
_Avoid_: Mode
