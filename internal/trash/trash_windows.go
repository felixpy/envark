//go:build windows

package trash

import (
	"fmt"
	"unsafe"

	"golang.org/x/sys/windows"
)

func move(path string) (string, error) {
	if Dir != "" {
		dest, err := unique(Dir, filepathBase(path))
		if err != nil {
			return "", err
		}
		if err := moveFile(path, dest); err != nil {
			return "", err
		}
		return dest, nil
	}
	// FO_DELETE with FOF_ALLOWUNDO sends the path to the Recycle Bin.
	// The shell does not tell us the recycled name, so restore is manual.
	from, err := windows.UTF16FromString(path)
	if err != nil {
		return "", err
	}
	from = append(from, 0)
	op := shFileOp{
		Func:  3,
		From:  &from[0],
		Flags: 0x40 | 0x10 | 0x400 | 0x4,
	}
	shell32 := windows.NewLazySystemDLL("shell32.dll")
	proc := shell32.NewProc("SHFileOperationW")
	r, _, _ := proc.Call(uintptr(unsafe.Pointer(&op)))
	if r != 0 {
		return "", fmt.Errorf("recycle bin returned %d", r)
	}
	return "", nil
}

type shFileOp struct {
	Hwnd          uintptr
	Func          uint32
	From          *uint16
	To            *uint16
	Flags         uint16
	AnyOps        int32
	NameMappings  uintptr
	ProgressTitle *uint16
}

func filepathBase(path string) string {
	for i := len(path) - 1; i >= 0; i-- {
		if path[i] == '\\' || path[i] == '/' {
			return path[i+1:]
		}
	}
	return path
}

func moveFile(from, to string) error {
	return windows.MoveFileEx(windows.StringToUTF16Ptr(from), windows.StringToUTF16Ptr(to), 0)
}
