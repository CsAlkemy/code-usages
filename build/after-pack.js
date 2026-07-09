'use strict';
// electron-builder afterPack hook: properly ad-hoc code-sign the whole .app
// bundle (seal resources, bind Info.plist, sign nested helpers/frameworks).
//
// Without a real Apple Developer ID we can't notarize, but a *valid* ad-hoc
// signature is what stops macOS (esp. Apple Silicon) from reporting the app as
// "damaged and can't be opened". Electron's binaries ship only a bare
// linker-signed stub (Identifier=Electron, resources not sealed), which reads
// as a broken signature once the app is quarantined on another Mac.
const { execSync } = require('child_process');
const path = require('path');

exports.default = async function afterPack(context) {
  if (context.electronPlatformName !== 'darwin') return;
  if (process.env.CSC_LINK) return; // real Developer ID signing follows — skip ad-hoc
  const app = path.join(context.appOutDir, `${context.packager.appInfo.productFilename}.app`);
  console.log('  • afterPack: ad-hoc signing', app);
  execSync(`codesign --force --deep --sign - --timestamp=none "${app}"`, { stdio: 'inherit' });
  execSync(`codesign --verify --deep --strict --verbose=1 "${app}"`, { stdio: 'inherit' });
};
