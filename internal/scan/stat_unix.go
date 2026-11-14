//go:build unix

package scan

import (
	"io/fs"
	"syscall"
)

func platformStat(info fs.FileInfo) fileStat {
	st, ok := info.Sys().(*syscall.Stat_t)
	if !ok {
		return fileStat{Alloc: uint64(max64(info.Size()))}
	}
	return fileStat{
		Dev:           uint64(st.Dev),
		Ino:           uint64(st.Ino),
		Alloc:         uint64(st.Blocks) * 512,
		Nlink:         uint64(st.Nlink),
		AllocExact:    true,
		HardlinkKnown: true,
	}
}
