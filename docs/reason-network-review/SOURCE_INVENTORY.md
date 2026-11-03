# 분석 대상과 파일 기준

작성일: 2026-10-22 (Asia/Seoul)

## 범위와 사용 방법

기준 commit: `f447dd3b2aee58887e16f628bb40b6e0d8012271`. 이 목록은 분석 대상을 재확인하기 위한 파일 목록입니다. 파일별 정밀 검토 완료를 뜻하지 않습니다. 주 보고서의 확인 범위와 한계를 함께 읽으십시오.

원본 Git 추적 파일 전체에 대해 파일 hash를 기록했습니다. 소스 38개(Rust/TypeScript, 테스트 코드 포함)에서 외부 접속·프로세스 실행·파일 접근·환경변수·내장 자산 경로를 검색하고, 주 보고서에 기재한 호출 경로를 상세 확인했습니다.

Cargo.lock 항목은 446개이며, 자체 package 1개와 crates.io registry package 445개입니다. git 또는 다른 registry source 항목은 없습니다. 모든 의존성 소스를 정밀 검토한 것은 아닙니다.

## 확보한 의존성 소스

아래 archive는 Cargo.lock의 정확한 version으로 static.crates.io에서 받았으며, SHA-256을 lock checksum과 대조했습니다. checksum 일치는 동일 자료 확보의 근거이며 악성 코드 부재의 증거는 아닙니다. 빌드·설치·테스트는 실행하지 않았습니다.

| archive | SHA-256 |
|---|---|
| [iroh-1.4.1.crate](dependency-sources/iroh-1.4.1.crate) | `e3689cab508af9bcef7816c1201b403595ec6aa20e7d15e1f9683bb4be4a5b17` |
| [iroh-base-1.4.1.crate](dependency-sources/iroh-base-1.4.1.crate) | `43b72366b16001af3a51574233df88ca6d62da0215807549a769e7de887da803` |
| [iroh-dns-1.4.1.crate](dependency-sources/iroh-dns-1.4.1.crate) | `7481d0792dde1035a2b06ec06ed3cf6d01fcb1a494e6931b489e32073382048d` |
| [iroh-relay-1.4.1.crate](dependency-sources/iroh-relay-1.4.1.crate) | `7e5589e7c1475700686603a970275687f760df853bc6525c7eb1347aa31d7230` |
| [iroh-mdns-address-lookup-0.6.0.crate](dependency-sources/iroh-mdns-address-lookup-0.6.0.crate) | `7afb03c296022fcb724a7bb76dbb8dcca6aac4f4eb65df161e084b7276c93d8c` |
| [iroh-metrics-1.0.2.crate](dependency-sources/iroh-metrics-1.0.2.crate) | `8ede55536349842337f7cdc63012ec33dc637945f3259c98a0763aac364cdf09` |
| [portmapper-0.19.3.crate](dependency-sources/portmapper-0.19.3.crate) | `ca97242f016e090a25330613bc2382aab65eb22f9149dbe84d8f8e711d49a530` |

네트워크 기본 설정·주소 게시·port mapping·metrics·일부 build script를 읽기 위해 선택한 파일만 dependency-excerpts 아래에 원문 그대로 복사했습니다. excerpt 확보 자체는 해당 package 전체 검토 완료를 뜻하지 않습니다.

## 원본 추적 파일

| 파일 | 바이트 수 | 작업 트리 SHA-256 |
|---|---:|---|
| `.git-blame-ignore-revs` | 152 | `a4e67b1a71133d1fd5f1cd81ff568198d221b0ee415f25fb038af700f8e9a5e4` |
| `.github/dependabot.yml` | 349 | `fb10b2f874be8d914601730c45101c6e4139f37d91e9938968b6de95979f72e8` |
| `.github/workflows/ci.yml` | 699 | `7f068497b51ddc190c19b893aa0e74e2810ff1fba920c74ee5852ba978ea89e9` |
| `.github/workflows/release.yml` | 3885 | `e7e8a74d17b65babe1270e64cebd1a4360cb3984e6ba9eaeade4bfa1417abee4` |
| `.gitignore` | 9 | `f61c42065f048ea8fa74560cdb2ce376ca780b0904950df8b77d7a1156185c7f` |
| `CONTRIBUTING.md` | 906 | `43f196b88010ebbafcf769909aec2bff7eb1f628e8c178395c14983d402867d1` |
| `Cargo.lock` | 117693 | `d0c0b472464b37b04fc7a930dfd7e1500f4ed6308e8fddf3cb1518df7622ed39` |
| `Cargo.toml` | 1926 | `bfbcf5f444f49dc1fd95474d586a5a657824bb25acc4f63cecb8ad585854b376` |
| `DESIGN.md` | 33889 | `43d90444d080935bc041e4041cd374a54f136b309923e74ed43fcc8fdb6e5cc7` |
| `LICENSE-APACHE` | 11560 | `3ddf9be5c28fe27dad143a5dc76eea25222ad1dd68934a047064e56ed2fa40c5` |
| `LICENSE-MIT` | 1101 | `649641010233d7cf1724d0dbf901b89192a78ecd503d8d56320809f90fa8b556` |
| `NOTICE` | 470 | `aa2dddb2710aea63bef3802bd67dab52514ced657dc1c395383c7c32b0ac7bfb` |
| `README.md` | 7707 | `dcf6cbc10569ea5bdfbc86c61b3087c3bbd4aecffa392a98e1838b9888a50b29` |
| `SECURITY.md` | 649 | `a983243e3dcb292d5e728b26e3a1004a3e80460785081cf4680bd0dbf3394e8d` |
| `assets/reason-network-banner.png` | 661933 | `338848f742d79ce791d9275e3b71de315e4fd8a9c985f85811efc6513fd356e5` |
| `assets/reason.icns` | 25646 | `5e341fa65687ecec46d65d04f06a47f8cb3d3fe07c89dba6962ab971e02cb695` |
| `install.ps1` | 4915 | `d02765f65ac4a421cc11a143c154d3f72d5228615633c9de706eb6b4887323c5` |
| `install.sh` | 6010 | `516c911ebbf094fbbb1ac4cfe262b52b78c404fa71cb42495cf0433803bdc737` |
| `rustfmt.toml` | 47 | `e27d70e4fa1a9ff63a14b5e53a26bc35c5a277a25fc967a4252cb3866a0e1394` |
| `src/agent.rs` | 10359 | `2daeb71374bc11c05a564d31d81b5c1b7e3203f8946fce8b816ad2d31ca1de95` |
| `src/claude.rs` | 7783 | `8fc23c4ef15d48e34d3c4bf74c66abfd766a8da4a98e901155a9a3279f6f04dc` |
| `src/client.rs` | 11159 | `0d17eea6418d14bf8cce0bf42b2e0040105416fd97f6cbdbc88c6bea63d9e32b` |
| `src/config.rs` | 14307 | `a849a79ab028afdb0fcb06879e76fe4e13bcfee01907489e2643d5864b7f72f1` |
| `src/daemon/ask.rs` | 28843 | `8e49abee831c4be5c16495a9566e24d2cf17815880a6686ca42bcb4205990c6d` |
| `src/daemon/directory.rs` | 21083 | `6d4133036e7bdd3ed00032695554353b49d114e97c3b2438c8410ad021df7d1d` |
| `src/daemon/mail.rs` | 45768 | `e3be22fb10df66543a257deb0e44ca60596182fa45b0f910897b2ae7c08e4e67` |
| `src/daemon/mod.rs` | 15977 | `729d23a2e8d3457df99eb4214876a7f9aa202e5c7bc8f48cab3d42856748e342` |
| `src/daemon/notify.rs` | 10483 | `f138e70791e6e1cab578143f33e740fc75b6a056fef0893beaa10c15167dfe74` |
| `src/daemon/reason.rs` | 12491 | `6ca7a794cf3f1e75dff886a79295fce21094d203340e63e48ec95d3d3c0b7cb9` |
| `src/daemon/sessions.rs` | 19706 | `4cbed604befc3098c171c34f49f43c6a1ba3b0adf27e668ce0922acf5dc149ec` |
| `src/daemon/teamops.rs` | 31618 | `7139dbc46a90a7531c56c6c38d743c5277727fda9be9b7fe598bd6c10d0a26aa` |
| `src/daemon/update.rs` | 5164 | `99757f38634e713df849bad44fe8e58c290ed3fea51e7d1504e7cd24dddd52db` |
| `src/doing.rs` | 11483 | `e65c05eb5920176c97f9aa6daf24c3b626d2124c15d59be18c118bb431433f06` |
| `src/files.rs` | 40316 | `10f509c56ae3b5c5388f436c58227c563e0a6ec4d2fb40220070e5caf7180752` |
| `src/git.rs` | 2463 | `c5c70f3a879c817cb743a35cf2f178e7eee8c934b86618726b14a21d88948200` |
| `src/hook.rs` | 9018 | `ec13bcebe7234b3b2a4391e96a4680a0373ff4ee827dd89eee1403afe83410b2` |
| `src/invite.rs` | 2680 | `c6db40a4f2a70e2dfb9484a4eabdb05387aab54079e384afd9bc67d48a0fbb22` |
| `src/ipc.rs` | 3432 | `05aa352adc96d710adde2c6ab69aef3c1fd8138c3f2d387b58621d1da385ab02` |
| `src/main.rs` | 39840 | `c7d6c13301967fc81aaeda6b4e4215584a029d148f70d9240c4b8fd26185d645` |
| `src/mcp.rs` | 17755 | `54b99994e51174c3685a3a04bcc99eaffc2b8e0bded118c581a119ddad74de7a` |
| `src/migrate.rs` | 4951 | `cad3d0b7d2755f06b2c0937b677958d7bbc48ffa57dbf222831058025818b5b0` |
| `src/net.rs` | 60799 | `966cc780e1855edd477a2d965d51f8434a0d77bcb0298b335895199d2253b469` |
| `src/opencode-plugin.ts` | 6102 | `c2bb22d44d22ac6f1502a51e483b4b672ce4f01dcc82a015eb6183545201e1ab` |
| `src/paths.rs` | 10423 | `385e49f3abc9e0819d7d5ea0d2e775c94699738f36148fa3baaeb15d0d1cdbe1` |
| `src/pi-extension.ts` | 2745 | `22e948d3579aedf4644bb601d6bfcb6c088ab8c27d566e6355ef9f8d56e3bd8d` |
| `src/proof.rs` | 21346 | `2663bb53eda466e7b69b36e850b08fa7d4ccb8a52c9fa9da571fe8d22a550ada` |
| `src/proto.rs` | 18679 | `585bf28cc9d556dc24016167770c3d9b12cb95154f75c4aa41e2978df851aa4e` |
| `src/relays.rs` | 20789 | `abd1babc37d665e2a220c376cdb958ce8a8f3c0649de1216cdf65eef3d04f957` |
| `src/render.rs` | 26974 | `eaa9cdab3131b9226b74aef658330888dba4d5fe008f88a82fae27e04df6a862` |
| `src/setup.rs` | 70476 | `0e0ff8cf44b9f60a109299c86210b66d3f165a487b60a8ed8984a79c2cb75f9d` |
| `src/setup/zcode.rs` | 8843 | `aa3cbe0a7d6b7bf19a1e94f2a6c1d2fee819d60dfbb3d8ff09014e1129df9945` |
| `src/skill.md` | 3002 | `944248129607c8414745933d70be91d2d0a3cd1c4b93c7475ca8f1f0a95cb2e5` |
| `src/store.rs` | 41567 | `ece3fd697a641b8c190bf2612452a1d85d57d8bf759730d55b2d8191551d7728` |
| `src/team.rs` | 58994 | `189475662994136a118d6a57ff574664c1885dfbd06efd11455b494be7927939` |
| `src/testdata/proof-forger.pk8` | 1217 | `028a4c633bc9c47f4267982f83f85cac41f85f421f5e673a3f6b98756141caff` |
| `src/testdata/proof-signer.pk8` | 1218 | `694406bbeedc466498bc9338497bf31c57b55c66a0c74b196dce3ce853a12749` |
| `src/tray.rs` | 11300 | `7622f14131076481d06ed850e9908b15595d2de56c7e219439b49fd6e524a230` |
| `src/util.rs` | 15569 | `cedef2d98384306776ae1eab5f3c622a55b1275cd092dc9da2246366f42a9bf2` |
| `src/wizard.rs` | 13492 | `bb02a8004b156fd2b3ae844b62ddd739a2fe38594192f1ae68eef44ce9904ce9` |
| `src/words.txt` | 8469 | `44b2aee619270ede2c927402f7ae58b55f13bba7178bddba3decd75863906017` |
| `tests/e2e/guided.py` | 7521 | `cd563e93368b73b23f97d1d09a16e2872a23240b580e58065544096b33b753ec` |
| `tests/e2e/hooks.py` | 16776 | `e01af5590475c851f75d7e9c309872a27a7be6d4f0d4b19d2ed2babfc561b6c6` |
| `tests/e2e/mixed_versions.py` | 9127 | `57614178de5e7e00892a5f097230c7fdb3fbb1c02517106899e7c2f36018ef45` |
| `tests/e2e/proofs.py` | 4387 | `7396984c2d5e89d4c8301ee9526127a88bdcd391232539be592ba264e668a91e` |
| `tests/e2e/regional_relays.py` | 6256 | `5d95b8d38c7a30f95c5ac9f92306a1983e046bfa44fd405eab5e27f6eb8e01ea` |
| `tests/e2e/two_machines.py` | 40468 | `8fcbb10ab5647e21f04bbce8bd6f4dd87dc6b782010561d4be1a87f99198479c` |
| `tests/install_script.rs` | 3470 | `bbbe6c7a5f3614b0963d458d187ddd7b17f2aa804783c8a57a6744022b4ea53f` |

작업 트리 hash는 Windows checkout의 줄바꿈 변환 영향을 받을 수 있습니다. 운영체제 간 소스 비교 기준은 commit과 Git blob이며, 위 hash는 이번 분석 파일을 재확인하는 데 사용합니다.
