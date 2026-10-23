// Extension은 표시 계층입니다. Rust Core를 직접 포함하거나 호출하지 않고 Local Core API로만 접속합니다 (Spec §17).
// Core와 주고받는 타입은 ./generated/의 생성 타입만 사용합니다 (Spec §3.4).

import * as vscode from 'vscode';

export function activate(context: vscode.ExtensionContext): void {
  const output = vscode.window.createOutputChannel('TelosWeaver');
  context.subscriptions.push(
    output,
    vscode.commands.registerCommand('telosWeaver.showOutput', () => output.show()),
  );
}

export function deactivate(): void {}
