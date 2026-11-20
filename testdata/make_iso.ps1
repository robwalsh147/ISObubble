param([string]$Src, [string]$Out, [int]$Fs = 3)
$fsi = New-Object -ComObject IMAPI2FS.MsftFileSystemImage
$fsi.FileSystemsToCreate = $Fs
$fsi.VolumeName = "BUBBLE_TEST"
$fsi.Root.AddTree($Src, $false)
$res = $fsi.CreateResultImage()
Add-Type -TypeDefinition @"
using System; using System.IO; using System.Runtime.InteropServices.ComTypes;
public static class IsoWriter { public static void Write(object s, string path, int bs, int blocks) { IStream st = (IStream)s; using (var fs = File.Create(path)) { byte[] buf = new byte[bs]; for (int i = 0; i < blocks; i++) { st.Read(buf, bs, IntPtr.Zero); fs.Write(buf, 0, bs); } } } }
"@
[IsoWriter]::Write($res.ImageStream, $Out, $res.BlockSize, $res.TotalBlocks)
"done"
