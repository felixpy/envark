//go:build windows

package scan

import (
	"io/fs"

	"golang.org/x/sys/windows"
)

func statPath(path string, info fs.FileInfo) fileStat {
	out := fileStat{Alloc: uint64(max64(info.Size()))}
	p, err := windows.UTF16PtrFromString(path)
	if err != nil {
		return out
	}
	h, err := windows.CreateFile(p, windows.FILE_READ_ATTRIBUTES,
		windows.FILE_SHARE_READ|windows.FILE_SHARE_WRITE|windows.FILE_SHARE_DELETE,
		nil, windows.OPEN_EXISTING, windows.FILE_FLAG_BACKUP_SEMANTICS|windows.FILE_FLAG_OPEN_REPARSE_POINT, 0)
	if err != nil {
		return out
	}
	defer windows.CloseHandle(h)
	var d windows.ByHandleFileInformation
	if err := windows.GetFileInformationByHandle(h, &d); err != nil {
		return out
	}
	out.Dev = uint64(d.VolumeSerialNumber)
	out.Ino = uint64(d.FileIndexHigh)<<32 | uint64(d.FileIndexLow)
	out.Nlink = uint64(d.NumberOfLinks)
	out.HardlinkKnown = true
	// This build does not read a separate allocation size.
	out.AllocExact = false
	return out
}
