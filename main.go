// Envark is a desktop tool for seeing what a development machine has
// installed, what uses disk, and what can be reclaimed.
package main

import (
	"log"
	"os"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"

	"github.com/felixpy/envark/internal/app"
	"github.com/felixpy/envark/internal/trash"
)

func main() {
	if dir := os.Getenv("ENVARK_TRASH_DIR"); dir != "" {
		trash.Dir = dir
	}
	application, err := app.Open(app.Config{
		DataDir: os.Getenv("ENVARK_DATA_DIR"),
		Home:    os.Getenv("ENVARK_HOME"),
		Path:    os.Getenv("ENVARK_PATH"),
	})
	if err != nil {
		log.Fatal(err)
	}
	mygo.App.WhenReady(func() {
		application.Attach(mygo.NewWindow(mygo.WindowOptions{
			Title:     "Envark",
			Width:     1280,
			Height:    800,
			MinWidth:  720,
			MinHeight: 640,
			Content:   ui.View(application.View),
		}))
		mygo.App.SetMenu(menu(application))
		application.ApplyTheme()
	})
	if err := mygo.App.Run(); err != nil {
		log.Fatal(err)
	}
}

func menu(application *app.App) *mygo.Menu {
	return mygo.NewMenu([]*mygo.MenuItem{
		{Role: mygo.RoleAppMenu},
		{Label: "File", Submenu: []*mygo.MenuItem{
			{Label: "Add folder", Accelerator: "CmdOrCtrl+O", Click: func(*mygo.MenuItem, *mygo.Window) { application.PickRoot() }},
			{Label: "Scan", Accelerator: "CmdOrCtrl+R", Click: func(*mygo.MenuItem, *mygo.Window) { application.Scan() }},
			{Type: mygo.MenuItemSeparator},
			{Role: mygo.RoleQuit},
		}},
		{Label: "Go", Submenu: []*mygo.MenuItem{
			{Label: "Space", Accelerator: "CmdOrCtrl+1", Click: func(*mygo.MenuItem, *mygo.Window) { application.Show("space") }},
			{Label: "Environments", Accelerator: "CmdOrCtrl+2", Click: func(*mygo.MenuItem, *mygo.Window) { application.Show("environments") }},
			{Label: "Review", Accelerator: "CmdOrCtrl+3", Click: func(*mygo.MenuItem, *mygo.Window) { application.Show("review") }},
			{Label: "History", Accelerator: "CmdOrCtrl+4", Click: func(*mygo.MenuItem, *mygo.Window) { application.Show("history") }},
			{Label: "Settings", Accelerator: "CmdOrCtrl+5", Click: func(*mygo.MenuItem, *mygo.Window) { application.Show("settings") }},
		}},
		{Role: mygo.RoleEditMenu},
		{Role: mygo.RoleWindowMenu},
		{Label: "Help", Submenu: []*mygo.MenuItem{
			{Label: "Keyboard shortcuts", Click: func(*mygo.MenuItem, *mygo.Window) { application.ShowHelp() }},
		}},
	})
}
