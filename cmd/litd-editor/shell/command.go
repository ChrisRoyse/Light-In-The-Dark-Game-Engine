package shell

import (
	"errors"
	"fmt"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

const CommandStackLimit = 256

type Command interface {
	Label() string
	Apply(*App) error
	Revert(*App) error
	Noop() bool
}

type CommandStack struct {
	limit int
	undo  []Command
	redo  []Command
}

type StackSnapshot struct {
	Limit      int      `json:"limit"`
	UndoDepth  int      `json:"undoDepth"`
	RedoDepth  int      `json:"redoDepth"`
	Undo       []string `json:"undo"`
	Redo       []string `json:"redo"`
	OldestUndo string   `json:"oldestUndo,omitempty"`
	NewestUndo string   `json:"newestUndo,omitempty"`
}

func NewCommandStack(limit int) *CommandStack {
	if limit <= 0 {
		limit = CommandStackLimit
	}
	return &CommandStack{limit: limit}
}

func (s *CommandStack) Execute(app *App, cmd Command) error {
	if s == nil {
		return fmt.Errorf("editor command stack: nil stack")
	}
	if cmd == nil {
		return fmt.Errorf("editor command stack: nil command")
	}
	if cmd.Noop() {
		return nil
	}
	if err := cmd.Apply(app); err != nil {
		return err
	}
	s.undo = append(s.undo, cmd)
	if len(s.undo) > s.limit {
		copy(s.undo, s.undo[len(s.undo)-s.limit:])
		s.undo = s.undo[:s.limit]
	}
	s.redo = s.redo[:0]
	return nil
}

func (s *CommandStack) RecordApplied(cmd Command) error {
	if s == nil {
		return fmt.Errorf("editor command stack: nil stack")
	}
	if cmd == nil {
		return fmt.Errorf("editor command stack: nil command")
	}
	if cmd.Noop() {
		return nil
	}
	s.undo = append(s.undo, cmd)
	if len(s.undo) > s.limit {
		copy(s.undo, s.undo[len(s.undo)-s.limit:])
		s.undo = s.undo[:s.limit]
	}
	s.redo = s.redo[:0]
	return nil
}

func (s *CommandStack) Undo(app *App) error {
	if s == nil {
		return fmt.Errorf("editor command stack: nil stack")
	}
	if len(s.undo) == 0 {
		return nil
	}
	idx := len(s.undo) - 1
	cmd := s.undo[idx]
	s.undo = s.undo[:idx]
	if err := cmd.Revert(app); err != nil {
		s.undo = append(s.undo, cmd)
		return err
	}
	s.redo = append(s.redo, cmd)
	return nil
}

func (s *CommandStack) Redo(app *App) error {
	if s == nil {
		return fmt.Errorf("editor command stack: nil stack")
	}
	if len(s.redo) == 0 {
		return nil
	}
	idx := len(s.redo) - 1
	cmd := s.redo[idx]
	s.redo = s.redo[:idx]
	if err := cmd.Apply(app); err != nil {
		s.redo = append(s.redo, cmd)
		return err
	}
	s.undo = append(s.undo, cmd)
	if len(s.undo) > s.limit {
		copy(s.undo, s.undo[len(s.undo)-s.limit:])
		s.undo = s.undo[:s.limit]
	}
	return nil
}

func (s *CommandStack) Snapshot() StackSnapshot {
	if s == nil {
		return StackSnapshot{}
	}
	snap := StackSnapshot{
		Limit:     s.limit,
		UndoDepth: len(s.undo),
		RedoDepth: len(s.redo),
		Undo:      commandLabels(s.undo),
		Redo:      commandLabels(s.redo),
	}
	if len(snap.Undo) > 0 {
		snap.OldestUndo = snap.Undo[0]
		snap.NewestUndo = snap.Undo[len(snap.Undo)-1]
	}
	return snap
}

func commandLabels(commands []Command) []string {
	labels := make([]string, len(commands))
	for i, cmd := range commands {
		labels[i] = cmd.Label()
	}
	return labels
}

type CompositeCommand struct {
	label    string
	commands []Command
}

func NewCompositeCommand(label string, commands []Command) (*CompositeCommand, error) {
	if label == "" {
		return nil, fmt.Errorf("editor command stack: composite command requires a label")
	}
	c := &CompositeCommand{label: label}
	for _, cmd := range commands {
		if cmd == nil {
			return nil, fmt.Errorf("editor command stack: composite command %q contains nil command", label)
		}
		if cmd.Noop() {
			continue
		}
		c.commands = append(c.commands, cmd)
	}
	return c, nil
}

func (c *CompositeCommand) Label() string { return c.label }

func (c *CompositeCommand) Apply(app *App) error {
	applied := 0
	for i, cmd := range c.commands {
		if err := cmd.Apply(app); err != nil {
			var rollbackErr error
			for j := applied - 1; j >= 0; j-- {
				if revertErr := c.commands[j].Revert(app); revertErr != nil {
					rollbackErr = errors.Join(rollbackErr, fmt.Errorf("%s rollback step %d: %w", c.label, j, revertErr))
				}
			}
			return errors.Join(fmt.Errorf("%s step %d: %w", c.label, i, err), rollbackErr)
		}
		applied++
	}
	return nil
}

func (c *CompositeCommand) Revert(app *App) error {
	for i := len(c.commands) - 1; i >= 0; i-- {
		if err := c.commands[i].Revert(app); err != nil {
			return fmt.Errorf("%s undo step %d: %w", c.label, i, err)
		}
	}
	return nil
}

func (c *CompositeCommand) Noop() bool { return len(c.commands) == 0 }

type gridCellCommand struct {
	kind          sourceform.GridKind
	x, y          int
	before, after int
}

func (c gridCellCommand) Label() string {
	return fmt.Sprintf("%s[%d,%d]:%d->%d", c.kind, c.x, c.y, c.before, c.after)
}

func (c gridCellCommand) Apply(app *App) error {
	return app.setGridCellDirect(c.kind, c.x, c.y, c.after)
}

func (c gridCellCommand) Revert(app *App) error {
	return app.setGridCellDirect(c.kind, c.x, c.y, c.before)
}

func (c gridCellCommand) Noop() bool { return c.before == c.after }

type cliffCellCommand struct {
	x, y          int
	before, after sourceform.CliffCell
}

func (c cliffCellCommand) Label() string {
	return fmt.Sprintf("cliff[%d,%d]:%s->%s", c.x, c.y, cliffCellLabel(c.before), cliffCellLabel(c.after))
}

func (c cliffCellCommand) Apply(app *App) error {
	return app.setCliffCellDirect(c.x, c.y, c.after)
}

func (c cliffCellCommand) Revert(app *App) error {
	return app.setCliffCellDirect(c.x, c.y, c.before)
}

func (c cliffCellCommand) Noop() bool { return c.before == c.after }

type entityMoveCommand struct {
	id                        uint32
	beforePos, afterPos       [2]int
	beforeFacing, afterFacing int
}

func (c entityMoveCommand) Label() string {
	return fmt.Sprintf("entity[%d]:pos(%d,%d)->(%d,%d),facing:%d->%d", c.id, c.beforePos[0], c.beforePos[1], c.afterPos[0], c.afterPos[1], c.beforeFacing, c.afterFacing)
}

func (c entityMoveCommand) Apply(app *App) error {
	return app.moveEntityDirect(c.id, c.afterPos, c.afterFacing)
}

func (c entityMoveCommand) Revert(app *App) error {
	return app.moveEntityDirect(c.id, c.beforePos, c.beforeFacing)
}

func (c entityMoveCommand) Noop() bool {
	return c.beforePos == c.afterPos && c.beforeFacing == c.afterFacing
}

type metadataNameCommand struct {
	before, after string
}

func (c metadataNameCommand) Label() string {
	return fmt.Sprintf("metadata.name:%q->%q", c.before, c.after)
}

func (c metadataNameCommand) Apply(app *App) error {
	return app.setMetadataNameDirect(c.after)
}

func (c metadataNameCommand) Revert(app *App) error {
	return app.setMetadataNameDirect(c.before)
}

func (c metadataNameCommand) Noop() bool { return c.before == c.after }
