//go:build !windows

package scan

import "io/fs"

func statPath(path string, info fs.FileInfo) fileStat {
	_ = path
	return platformStat(info)
}
